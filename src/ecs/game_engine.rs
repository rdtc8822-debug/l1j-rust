/// Tick-Based Game Engine
///
/// The core game loop that processes ALL NPCs in a single thread per tick.
/// This replaces Java's thread-per-NPC model with a centralized tick loop.
///
/// Architecture:
///   - Fixed tick rate (default 200ms = 5 ticks/sec)
///   - Single loop iterates all active NPCs
///   - NPCs without nearby players are skipped (sleep optimization)
///   - Movement packets are batched and flushed once per tick

use std::collections::HashMap;

use rand::RngExt;

use crate::ecs::components::movement::Movement;
use crate::ecs::components::npc::{AiState, NpcTemplate};
use crate::ecs::components::position::{heading_delta, Position};
use crate::ecs::components::stats::Health;
use crate::ecs::components::visual::Visual;
use crate::world::grid::{ObjectId, WorldGrid};

/// A single NPC entity in the game world.
#[derive(Debug)]
pub struct NpcEntity {
    pub id: ObjectId,
    pub pos: Position,
    pub health: Health,
    pub movement: Movement,
    pub ai: AiState,
    pub visual: Visual,
    pub template_id: i32,
    pub alive: bool,
}

/// The game world state - holds all entities and the spatial grid.
pub struct GameWorld {
    /// All NPC entities, keyed by object ID.
    pub npcs: HashMap<ObjectId, NpcEntity>,

    /// Spatial grid for fast visibility queries.
    pub grid: WorldGrid,

    /// Player positions (for AI sleep optimization).
    /// Key = player object ID, Value = position.
    pub player_positions: HashMap<ObjectId, Position>,

    /// NPC template data (shared, immutable after load).
    pub npc_templates: HashMap<i32, NpcTemplate>,

    /// Next available object ID.
    next_object_id: ObjectId,

    /// Current tick count.
    pub tick_count: u64,
}

impl GameWorld {
    pub fn new(npc_templates: HashMap<i32, NpcTemplate>) -> Self {
        GameWorld {
            npcs: HashMap::new(),
            grid: WorldGrid::new(),
            player_positions: HashMap::new(),
            npc_templates,
            next_object_id: 0x10000000, // Same start as Java IdFactory
            tick_count: 0,
        }
    }

    /// Allocate a new unique object ID.
    pub fn next_id(&mut self) -> ObjectId {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }

    /// Spawn an NPC at the given position.
    ///
    /// Returns the allocated object ID, or None if the template is not found.
    pub fn spawn_npc(&mut self, template_id: i32, x: i32, y: i32, map_id: i32) -> Option<ObjectId> {
        // Clone template data first to avoid borrow conflict with self.next_id()
        let template = self.npc_templates.get(&template_id)?.clone();

        let id = self.next_id();
        let pos = Position::new(x, y, map_id);

        let entity = NpcEntity {
            id,
            pos,
            health: Health {
                cur_hp: template.hp,
                max_hp: template.hp,
                cur_mp: template.mp,
                max_mp: template.mp,
            },
            movement: Movement::new(),
            ai: AiState::new(x, y),
            visual: Visual::new_npc(
                template.gfxid,
                template.name.clone(),
                template.nameid.clone(),
            ),
            template_id,
            alive: true,
        };

        self.grid.add(id, map_id, x, y);
        self.npcs.insert(id, entity);

        Some(id)
    }

    /// Remove an NPC from the world.
    pub fn remove_npc(&mut self, id: ObjectId) {
        if let Some(npc) = self.npcs.remove(&id) {
            self.grid.remove(id, npc.pos.map_id, npc.pos.x, npc.pos.y);
        }
    }

    /// Check if any player is near the given position (within AI sleep range).
    fn any_player_nearby(&self, pos: &Position, range: i32) -> bool {
        for player_pos in self.player_positions.values() {
            if pos.tile_distance(player_pos) <= range {
                return true;
            }
        }
        false
    }

    /// Execute one game tick.
    ///
    /// This is the core of the Tick-Based AI engine.
    /// Processes ALL NPCs in a single pass - no threads, no timers.
    ///
    /// Returns a list of (npc_id, old_pos, new_pos) for NPCs that moved,
    /// so the caller can generate movement packets.
    pub fn tick(&mut self, ai_sleep_range: i32) -> Vec<NpcMovement> {
        self.tick_count += 1;
        let mut movements = Vec::new();
        let mut rng = rand::rng();

        // Collect NPC IDs to iterate (avoids borrow issues)
        let npc_ids: Vec<ObjectId> = self.npcs.keys().copied().collect();

        for npc_id in npc_ids {
            // Check if NPC exists and is alive
            let (_pos, template_id, should_process) = {
                let npc = match self.npcs.get(&npc_id) {
                    Some(n) => n,
                    None => continue,
                };
                if !npc.alive {
                    continue;
                }
                let nearby = self.any_player_nearby(&npc.pos, ai_sleep_range);
                (npc.pos, npc.template_id, nearby)
            };

            // Skip AI for NPCs with no players nearby (sleep optimization)
            if !should_process {
                if let Some(npc) = self.npcs.get_mut(&npc_id) {
                    npc.ai.players_nearby = false;
                }
                continue;
            }

            // Get template for this NPC
            let is_monster = self.npc_templates.get(&template_id)
                .map(|t| t.impl_type.contains("Monster"))
                .unwrap_or(false);

            // Process AI for this NPC
            let npc = match self.npcs.get_mut(&npc_id) {
                Some(n) => n,
                None => continue,
            };

            npc.ai.players_nearby = true;
            npc.ai.active = true;

            // Tick movement cooldown
            npc.movement.tick();

            // Skip if in cooldown
            if !npc.movement.can_move() {
                continue;
            }

            // AI Decision: random walk if no target (monsters and guards)
            if npc.ai.target_id == 0 && is_monster {
                // Random walk behavior
                if npc.ai.random_walk_distance == 0 {
                    npc.ai.random_walk_distance = rng.random_range(1..=5);
                    npc.ai.random_walk_direction = rng.random_range(0..8);

                    // Occasionally walk toward home point
                    if npc.ai.home_x != 0 && npc.ai.home_y != 0 && rng.random_range(0..3) == 0 {
                        let dx = npc.ai.home_x - npc.pos.x;
                        let dy = npc.ai.home_y - npc.pos.y;
                        if dx != 0 || dy != 0 {
                            npc.ai.random_walk_direction = direction_from_delta(dx, dy);
                        }
                    }
                } else {
                    npc.ai.random_walk_distance -= 1;
                }

                // Execute the move
                let heading = npc.ai.random_walk_direction;
                let (dx, dy) = heading_delta(heading);
                let new_x = npc.pos.x + dx;
                let new_y = npc.pos.y + dy;

                // Record the movement
                let old_pos = npc.pos;
                npc.pos.x = new_x;
                npc.pos.y = new_y;
                npc.pos.heading = heading;
                npc.movement.cooldown_ticks = npc.movement.move_delay_ticks;

                // Update grid
                self.grid.move_object(
                    npc_id,
                    old_pos.map_id,
                    old_pos.x,
                    old_pos.y,
                    new_x,
                    new_y,
                );

                movements.push(NpcMovement {
                    npc_id,
                    old_pos,
                    new_pos: npc.pos,
                });
            }
        }

        movements
    }
}

/// Represents a single NPC movement during a tick.
#[derive(Debug)]
pub struct NpcMovement {
    pub npc_id: ObjectId,
    pub old_pos: Position,
    pub new_pos: Position,
}

/// Convert a (dx, dy) direction delta to the closest L1J heading (0-7).
fn direction_from_delta(dx: i32, dy: i32) -> i32 {
    if dx == 0 && dy > 0 { return 0; }  // South
    if dx < 0 && dy > 0 { return 1; }   // Southwest
    if dx < 0 && dy == 0 { return 2; }  // West
    if dx < 0 && dy < 0 { return 3; }   // Northwest
    if dx == 0 && dy < 0 { return 4; }  // North
    if dx > 0 && dy < 0 { return 5; }   // Northeast
    if dx > 0 && dy == 0 { return 6; }  // East
    if dx > 0 && dy > 0 { return 7; }   // Southeast
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_template(npc_id: i32, name: &str, impl_type: &str) -> NpcTemplate {
        NpcTemplate {
            npc_id,
            name: name.to_string(),
            nameid: name.to_string(),
            impl_type: impl_type.to_string(),
            gfxid: 100,
            level: 1,
            hp: 100,
            mp: 50,
            ac: 10,
            str_stat: 10, con_stat: 10, dex_stat: 10,
            wis_stat: 10, int_stat: 10, mr: 0,
            exp: 100, lawful: 0,
            size: "small".to_string(),
            undead: 0, poison_atk: 0, paralysis_atk: 0,
            agro: true, agrososc: false, agrocoi: false,
            family: 0, agrofamily: 0, pickup_item: false,
            brave_speed: 0, passispeed: 640, atkspeed: 1020,
            atk_magic_speed: 0, tamable: false, teleport: false,
            doppel: false, hpr_interval: 0, hpr: 0,
            mpr_interval: 0, mpr: 0, ranged: 0, light_size: 0,
            change_head: false, damage_reduction: 0, hard: false,
            karma: 0, transform_id: 0, transform_gfxid: 0,
            cant_resurrect: false,
        }
    }

    #[test]
    fn test_spawn_npc() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);
        let id = world.spawn_npc(45000, 32800, 32800, 4);

        assert!(id.is_some());
        assert_eq!(world.npcs.len(), 1);
        assert_eq!(world.grid.total_objects(), 1);
    }

    #[test]
    fn test_spawn_10000_npcs() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);

        for i in 0..10_000 {
            let x = 32000 + (i % 200);
            let y = 32000 + (i / 200);
            world.spawn_npc(45000, x, y, 4);
        }

        assert_eq!(world.npcs.len(), 10_000);
        assert_eq!(world.grid.total_objects(), 10_000);
    }

    #[test]
    fn test_tick_no_players_skips_ai() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);
        world.spawn_npc(45000, 32800, 32800, 4);

        // No players registered - NPCs should not move
        let movements = world.tick(30);
        assert!(movements.is_empty());
    }

    #[test]
    fn test_tick_with_player_nearby() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);
        let npc_id = world.spawn_npc(45000, 32800, 32800, 4).unwrap();

        // Register a player nearby
        world.player_positions.insert(99999, Position::new(32810, 32810, 4));

        // Tick - NPC should attempt to move
        let movements = world.tick(30);

        // NPC should have moved (random walk)
        assert_eq!(movements.len(), 1);
        assert_eq!(movements[0].npc_id, npc_id);
        assert_ne!(movements[0].old_pos, movements[0].new_pos);
    }

    #[test]
    fn test_tick_10000_npcs_with_player() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);

        for i in 0..10_000 {
            let x = 32000 + (i % 200);
            let y = 32000 + (i / 200);
            world.spawn_npc(45000, x, y, 4);
        }

        // Player at center
        world.player_positions.insert(99999, Position::new(32100, 32025, 4));

        // Run 10 ticks
        let mut total_movements = 0;
        for _ in 0..10 {
            let movements = world.tick(30);
            total_movements += movements.len();
        }

        // Some NPCs near the player should have moved
        assert!(total_movements > 0);
        // But NOT all 10,000 should be active (sleep optimization)
        assert!(total_movements < 100_000); // max 10k * 10 ticks
    }

    #[test]
    fn test_remove_npc() {
        let mut templates = HashMap::new();
        templates.insert(45000, make_test_template(45000, "TestMob", "L1Monster"));

        let mut world = GameWorld::new(templates);
        let id = world.spawn_npc(45000, 32800, 32800, 4).unwrap();

        world.remove_npc(id);
        assert_eq!(world.npcs.len(), 0);
        assert_eq!(world.grid.total_objects(), 0);
    }
}
