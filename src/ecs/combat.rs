/// Combat calculation system.
///
/// Ported from Java L1Attack.java. Handles hit/miss determination
/// and damage calculation for melee and ranged attacks.

use rand::{Rng, RngExt};

/// Attack types for calculation branching.
#[derive(Debug, Clone, Copy)]
pub enum AttackType {
    PcVsPc,
    PcVsNpc,
    NpcVsPc,
    NpcVsNpc,
}

/// Attacker stats needed for combat calculation.
#[derive(Debug, Clone)]
pub struct AttackerStats {
    pub level: i32,
    pub str_stat: i32,
    pub dex_stat: i32,
    pub hit_modifier: i32,     // weapon hit bonus
    pub dmg_modifier: i32,     // weapon damage bonus
    pub weapon_max_damage: i32,
    pub weapon_enchant: i32,
    pub is_ranged: bool,
}

/// Defender stats needed for combat calculation.
#[derive(Debug, Clone)]
pub struct DefenderStats {
    pub level: i32,
    pub ac: i32,
    pub dex_stat: i32,
    pub mr: i32,
    pub damage_reduction: i32,
    pub cur_hp: i32,
    pub max_hp: i32,
}

/// Result of a single attack calculation.
#[derive(Debug)]
pub struct AttackResult {
    pub hit: bool,
    pub damage: i32,
    pub is_critical: bool,
}

/// Calculate a melee/ranged attack.
///
/// Simplified from Java L1Attack. The core formula:
///   Hit roll: d20 + attacker_hit_bonus vs 10 + defender_dodge
///   Damage:   weapon_damage + str_bonus + enchant - armor_reduction
pub fn calculate_attack(
    attacker: &AttackerStats,
    defender: &DefenderStats,
    attack_type: AttackType,
) -> AttackResult {
    let mut rng = rand::rng();

    // Hit calculation
    let hit = calc_hit(&mut rng, attacker, defender, attack_type);

    if !hit {
        return AttackResult {
            hit: false,
            damage: 0,
            is_critical: false,
        };
    }

    // Damage calculation
    let (damage, is_critical) = calc_damage(&mut rng, attacker, defender, attack_type);

    AttackResult {
        hit: true,
        damage: damage.max(0),
        is_critical,
    }
}

/// Hit roll calculation.
fn calc_hit(
    rng: &mut impl Rng,
    attacker: &AttackerStats,
    defender: &DefenderStats,
    _attack_type: AttackType,
) -> bool {
    // Attacker roll: d20 + hit_modifier + (STR or DEX bonus) + level/2
    let stat_bonus = if attacker.is_ranged {
        (attacker.dex_stat - 10) / 2
    } else {
        (attacker.str_stat - 10) / 2
    };

    let attacker_roll = rng.random_range(1..=20)
        + attacker.hit_modifier
        + stat_bonus
        + attacker.level / 2;

    // Defender dodge: 10 + AC + DEX bonus
    let defender_dodge = 10 - defender.ac + (defender.dex_stat - 10) / 3;

    attacker_roll >= defender_dodge
}

/// Damage calculation.
fn calc_damage(
    rng: &mut impl Rng,
    attacker: &AttackerStats,
    defender: &DefenderStats,
    _attack_type: AttackType,
) -> (i32, bool) {
    // Base weapon damage
    let weapon_damage = if attacker.weapon_max_damage > 0 {
        rng.random_range(1..=attacker.weapon_max_damage)
    } else {
        rng.random_range(1..=4) // unarmed
    };

    // STR/DEX bonus
    let stat_bonus = if attacker.is_ranged {
        (attacker.dex_stat - 10) / 2
    } else {
        (attacker.str_stat - 10) / 2
    };

    // Enchant bonus
    let enchant_bonus = attacker.weapon_enchant;

    // Critical hit (5% chance, double damage)
    let is_critical = rng.random_range(1..=20) == 20;
    let crit_multiplier = if is_critical { 2 } else { 1 };

    // Total damage
    let raw_damage = (weapon_damage + attacker.dmg_modifier + stat_bonus + enchant_bonus)
        * crit_multiplier;

    // Armor reduction (simplified: higher level = better reduction)
    let reduction = defender.damage_reduction + (defender.ac.abs() / 3);

    let final_damage = (raw_damage - reduction).max(1); // minimum 1 damage

    (final_damage, is_critical)
}

/// Calculate NPC auto-attack damage (NPC vs PC or NPC vs NPC).
///
/// Simplified from Java L1Attack.calcNpcPcDamage():
///   damage = random(level) + STR/2 + 1 + modifiers
pub fn calculate_npc_attack(
    npc_level: i32,
    npc_str: i32,
    defender: &DefenderStats,
) -> AttackResult {
    let mut rng = rand::rng();

    // Simple hit roll
    let hit_roll = rng.random_range(1..=20) + npc_level / 2;
    let dodge = 10 - defender.ac;

    if hit_roll < dodge {
        return AttackResult {
            hit: false,
            damage: 0,
            is_critical: false,
        };
    }

    // NPC damage formula
    let base_damage = if npc_level > 0 {
        rng.random_range(0..npc_level)
    } else {
        0
    };
    let damage = base_damage + npc_str / 2 + 1 - defender.damage_reduction;

    AttackResult {
        hit: true,
        damage: damage.max(1),
        is_critical: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attack_always_does_min_1_damage() {
        let attacker = AttackerStats {
            level: 1, str_stat: 10, dex_stat: 10,
            hit_modifier: 100, // guaranteed hit
            dmg_modifier: 0, weapon_max_damage: 1,
            weapon_enchant: 0, is_ranged: false,
        };
        let defender = DefenderStats {
            level: 99, ac: -50, dex_stat: 30, mr: 100,
            damage_reduction: 100, cur_hp: 9999, max_hp: 9999,
        };

        let result = calculate_attack(&attacker, &defender, AttackType::PcVsNpc);
        if result.hit {
            assert!(result.damage >= 1, "Minimum damage should be 1");
        }
    }

    #[test]
    fn test_high_stats_attacker() {
        let attacker = AttackerStats {
            level: 50, str_stat: 30, dex_stat: 20,
            hit_modifier: 10, dmg_modifier: 15,
            weapon_max_damage: 20, weapon_enchant: 7,
            is_ranged: false,
        };
        let defender = DefenderStats {
            level: 10, ac: 5, dex_stat: 10, mr: 0,
            damage_reduction: 0, cur_hp: 100, max_hp: 100,
        };

        // Run 100 attacks, most should hit with decent damage
        let mut hits = 0;
        let mut total_damage = 0;
        for _ in 0..100 {
            let result = calculate_attack(&attacker, &defender, AttackType::PcVsNpc);
            if result.hit {
                hits += 1;
                total_damage += result.damage;
            }
        }

        assert!(hits > 50, "High-stat attacker should hit more than 50% of the time");
        assert!(total_damage > 0, "Should deal some damage");
    }

    #[test]
    fn test_npc_attack() {
        let defender = DefenderStats {
            level: 10, ac: 0, dex_stat: 12, mr: 0,
            damage_reduction: 0, cur_hp: 200, max_hp: 200,
        };

        let result = calculate_npc_attack(20, 14, &defender);
        // Just verify it doesn't panic
        assert!(result.damage >= 0 || !result.hit);
    }
}
