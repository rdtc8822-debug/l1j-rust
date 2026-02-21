/// Skill Execution Engine.
///
/// This is the "chef" that turns skill data (recipes) into actual game effects.
///
/// Execution flow (ported from Java L1SkillUse.java):
///   1. Validate: check MP/HP/reagents/cooldown/target
///   2. Consume: deduct MP/HP/reagents
///   3. Calculate: damage or effect magnitude
///   4. Apply: deal damage, add buffs, heal
///   5. Broadcast: send animation/effect packets to nearby players
///   6. Cooldown: set skill delay

use rand::RngExt;

use crate::ecs::components::skill::{SkillEffects, SkillCooldowns, SkillTemplate};

// ===========================================================================
// Skill execution context
// ===========================================================================

/// Everything needed about the caster to execute a skill.
#[derive(Debug, Clone)]
pub struct CasterInfo {
    pub object_id: u32,
    pub x: i32,
    pub y: i32,
    pub map_id: i32,
    pub heading: i32,
    pub level: i32,
    pub cur_hp: i32,
    pub cur_mp: i32,
    pub int_stat: i32,      // affects damage and MP reduction
    pub sp_bonus: i32,      // spell power from equipment
    pub class_type: i32,    // CharClass enum value
}

/// Everything needed about a target.
#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub object_id: u32,
    pub x: i32,
    pub y: i32,
    pub map_id: i32,
    pub level: i32,
    pub cur_hp: i32,
    pub max_hp: i32,
    pub cur_mp: i32,
    pub mr: i32,            // magic resistance
    pub is_undead: bool,
}

/// Result of a skill execution attempt.
#[derive(Debug)]
pub enum SkillResult {
    /// Skill succeeded - contains effects to apply.
    Success(SkillOutcome),
    /// Not enough MP.
    InsufficientMp,
    /// Not enough HP.
    InsufficientHp,
    /// Skill on cooldown.
    OnCooldown { ticks_left: u32 },
    /// Caster level too low.
    LevelTooLow,
    /// Target out of range.
    OutOfRange,
    /// Target resisted (MR check failed).
    Resisted,
    /// Counter Magic blocked the spell.
    CounterMagic,
    /// No valid target.
    NoTarget,
}

/// What effects to apply after successful skill use.
#[derive(Debug)]
pub struct SkillOutcome {
    /// Damage to deal to target(s). Negative = healing.
    pub damage: Vec<(u32, i32)>,  // (target_id, damage)
    /// Buffs to add. (target_id, skill_id, duration_ticks, value)
    pub buffs: Vec<(u32, i32, u32, i32)>,
    /// MP consumed from caster.
    pub mp_consumed: i32,
    /// HP consumed from caster.
    pub hp_consumed: i32,
    /// GFX animation ID to play.
    pub gfx_id: i32,
    /// Whether this is an AoE skill.
    pub is_aoe: bool,
    /// Skill ID for cooldown tracking.
    pub skill_id: i32,
    /// Cooldown ticks to set.
    pub cooldown_ticks: u32,
}

// ===========================================================================
// Execution
// ===========================================================================

/// Execute a skill.
///
/// This is the main entry point - validates, calculates, and returns
/// the outcome to apply. The caller handles packet sending and state mutation.
pub fn execute_skill(
    skill: &SkillTemplate,
    caster: &CasterInfo,
    targets: &[TargetInfo],
    cooldowns: &SkillCooldowns,
    _caster_effects: &SkillEffects,
) -> SkillResult {
    // 1. Cooldown check
    if !cooldowns.is_ready(skill.skill_id) {
        let ticks = cooldowns.cooldowns.get(&skill.skill_id).copied().unwrap_or(0);
        return SkillResult::OnCooldown { ticks_left: ticks };
    }

    // 2. Calculate actual MP cost (reduced by INT)
    let mp_cost = calc_mp_cost(skill.mp_consume, caster.int_stat);
    let hp_cost = skill.hp_consume;

    // 3. Resource check
    if caster.cur_mp < mp_cost {
        return SkillResult::InsufficientMp;
    }
    if caster.cur_hp <= hp_cost {
        return SkillResult::InsufficientHp;
    }

    // 4. Level check
    if caster.level < skill.skill_level {
        return SkillResult::LevelTooLow;
    }

    // 5. Target check
    if targets.is_empty() && skill.target_to != 0 {
        return SkillResult::NoTarget;
    }

    // 6. Calculate effects per target
    let mut damage_list = Vec::new();
    let mut buff_list = Vec::new();
    let mut any_hit = false;

    for target in targets {
        // Range check
        let dist = ((caster.x - target.x).abs()).max((caster.y - target.y).abs());
        if skill.range > 0 && dist > skill.range {
            continue;
        }

        // Counter Magic check
        // (simplified: check if target has counter magic buff active)

        if skill.damage_value > 0 || skill.damage_dice > 0 {
            // Attack spell - calculate magic damage
            let mr_result = check_magic_resist(caster.level, target.level, target.mr);
            if !mr_result {
                continue; // resisted
            }

            let damage = calc_magic_damage(skill, caster);

            // Undead + healing = damage
            let final_damage = if target.is_undead && damage < 0 {
                -damage // healing becomes damage on undead
            } else {
                damage
            };

            damage_list.push((target.object_id, final_damage));
            any_hit = true;

        } else if skill.buff_duration > 0 {
            // Buff/debuff spell
            if skill.probability_value > 0 {
                // Probability-based debuff (e.g., stun, sleep)
                let mr_result = check_magic_resist(caster.level, target.level, target.mr);
                if !mr_result {
                    continue;
                }
            }

            let duration_ticks = (skill.buff_duration as u32) * 5; // seconds → ticks
            buff_list.push((target.object_id, skill.skill_id, duration_ticks, skill.damage_value));
            any_hit = true;

        } else if skill.damage_value < 0 {
            // Healing spell
            let heal = calc_healing(skill, caster);
            damage_list.push((target.object_id, -heal)); // negative damage = healing
            any_hit = true;
        }
    }

    // Self-buff (no target needed)
    if targets.is_empty() && skill.buff_duration > 0 && skill.target_to == 0 {
        let duration_ticks = (skill.buff_duration as u32) * 5;
        buff_list.push((caster.object_id as u32, skill.skill_id, duration_ticks, skill.damage_value));
        any_hit = true;
    }

    if !any_hit && !targets.is_empty() {
        return SkillResult::Resisted;
    }

    // Calculate cooldown
    let cooldown_ticks = if skill.reuse_delay > 0 {
        (skill.reuse_delay as u32) / 200 // ms → ticks
    } else {
        0
    };

    SkillResult::Success(SkillOutcome {
        damage: damage_list,
        buffs: buff_list,
        mp_consumed: mp_cost,
        hp_consumed: hp_cost,
        gfx_id: skill.cast_gfx,
        is_aoe: skill.area > 0,
        skill_id: skill.skill_id,
        cooldown_ticks,
    })
}

// ===========================================================================
// Calculation helpers
// ===========================================================================

/// Calculate actual MP cost after INT reduction.
///
/// Official: INT 13-17 → 1 MP reduction, INT 18+ → 2 MP reduction.
fn calc_mp_cost(base_cost: i32, int_stat: i32) -> i32 {
    let reduction = if int_stat >= 18 { 2 }
                    else if int_stat >= 13 { 1 }
                    else { 0 };
    (base_cost - reduction).max(1)
}

/// Calculate magic damage for attack spells.
///
/// Formula (simplified from Java L1Magic.calcMagicDamage):
///   base = damage_value + random(1..=damage_dice) * damage_dice_count
///   bonus = SP bonus + INT bonus
///   total = base + bonus
fn calc_magic_damage(skill: &SkillTemplate, caster: &CasterInfo) -> i32 {
    let mut rng = rand::rng();

    let mut damage = skill.damage_value;

    // Dice damage
    if skill.damage_dice > 0 && skill.damage_dice_count > 0 {
        for _ in 0..skill.damage_dice_count {
            damage += rng.random_range(1..=skill.damage_dice);
        }
    }

    // INT bonus (every 2 INT above 12 → +1 damage)
    let int_bonus = ((caster.int_stat - 12).max(0)) / 2;
    damage += int_bonus;

    // SP bonus from equipment
    damage += caster.sp_bonus;

    damage.max(0)
}

/// Calculate healing amount.
///
/// Formula: base_value + random dice + INT bonus
fn calc_healing(skill: &SkillTemplate, caster: &CasterInfo) -> i32 {
    let mut rng = rand::rng();

    let mut heal = skill.damage_value.abs();

    if skill.damage_dice > 0 {
        for _ in 0..skill.damage_dice_count.max(1) {
            heal += rng.random_range(1..=skill.damage_dice);
        }
    }

    // INT bonus
    let int_bonus = ((caster.int_stat - 12).max(0)) / 2;
    heal += int_bonus;

    heal.max(1)
}

/// Check if a spell penetrates magic resistance.
///
/// Official formula (simplified):
///   hit_rate = 90 - (MR - caster_level) + (caster_level - target_level) * 2
///   Clamped to 10%-95%.
fn check_magic_resist(caster_level: i32, target_level: i32, target_mr: i32) -> bool {
    let mut rng = rand::rng();

    let base = 90;
    let mr_penalty = target_mr.max(0);
    let level_bonus = (caster_level - target_level) * 2;

    let hit_rate = (base - mr_penalty + level_bonus).clamp(10, 95);
    let roll = rng.random_range(1..=100);

    roll <= hit_rate
}

// ===========================================================================
// Buff integration with combat
// ===========================================================================

/// Calculate damage modifier from active skill effects.
///
/// Checks for effects like Burning Spirit, Armor Break, etc.
pub fn calc_buff_damage_modifier(effects: &SkillEffects) -> f32 {
    let mut modifier = 1.0f32;

    // 燃燒鬥志 (skill 102): 34% chance × 1.5 damage
    if effects.has_effect(102) {
        let mut rng = rand::rng();
        if rng.random_range(0..100) < 34 {
            modifier *= 1.5;
        }
    }

    // 雙重破壞 (skill 105): 32% chance × 2.0 damage (requires dual sword/claw)
    if effects.has_effect(105) {
        let mut rng = rand::rng();
        if rng.random_range(0..100) < 32 {
            modifier *= 2.0;
        }
    }

    // 暗影之牙 (skill 107): +5 flat damage → represented as small multiplier
    // (flat bonuses applied separately in combat.rs)

    // 勇猛意志 (skill 117): 30% chance × 1.5 damage
    if effects.has_effect(117) {
        let mut rng = rand::rng();
        if rng.random_range(0..100) < 30 {
            modifier *= 1.5;
        }
    }

    modifier
}

/// Calculate flat damage bonus from active buffs.
pub fn calc_buff_flat_bonus(effects: &SkillEffects) -> i32 {
    let mut bonus = 0;

    // 暗影之牙 (107): +5 damage
    if effects.has_effect(107) { bonus += 5; }

    // 灼熱武器 (114): +5 hit, +5 damage
    if effects.has_effect(114) { bonus += 5; }

    // 擬似魔法武器 (12): +2 damage
    if effects.has_effect(12) { bonus += 2; }

    // 勇猛武器 (royal 70級): +10 damage
    if effects.has_effect(119) { bonus += 10; }

    bonus
}

/// Calculate AC modifier from active debuffs on target.
pub fn calc_debuff_ac_modifier(target_effects: &SkillEffects) -> i32 {
    let mut ac_change = 0;

    // 護衛毀滅 (142): AC -10
    if target_effects.has_effect(142) { ac_change -= 10; }

    // 精準目標 (113): damage reduction -3
    if target_effects.has_effect(113) { ac_change -= 3; }

    ac_change
}

/// Calculate damage increase when target has Armor Break debuff.
///
/// Official: +58% damage for 8 seconds.
pub fn calc_armor_break_multiplier(target_effects: &SkillEffects) -> f32 {
    if target_effects.has_effect(112) { // 破壞盔甲
        1.58
    } else {
        1.0
    }
}

/// Check if target is stunned/sleeping/paralyzed (cannot act).
pub fn is_incapacitated(effects: &SkillEffects) -> bool {
    // 衝擊之暈 (120)
    effects.has_effect(120)
    // 暗黑盲咒 (103) - sleep
    || effects.has_effect(103)
    // 幻想 (153) - petrify
    || effects.has_effect(153)
}

/// Check if target is silenced (cannot use magic).
pub fn is_silenced(effects: &SkillEffects) -> bool {
    // 封印禁地 (132)
    effects.has_effect(132)
    // 混亂 (154)
    || effects.has_effect(154)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::skill::{SkillEffects, SkillCooldowns, SkillTemplate};

    fn make_test_skill() -> SkillTemplate {
        SkillTemplate {
            skill_id: 17, name: "火球術".into(), skill_level: 4,
            skill_number: 17, mp_consume: 10, hp_consume: 0,
            item_consume_id: 0, item_consume_count: 0,
            reuse_delay: 0, buff_duration: 0,
            target: "attack".into(), target_to: 1,
            damage_value: 6, damage_dice: 6, damage_dice_count: 2,
            probability_value: 0, attr: 2, skill_type: 0,
            is_through: false, range: 10, area: 0,
            action_id: 19, cast_gfx: 1505, cast_gfx2: 0,
            sys_msg_id_happen: 0, sys_msg_id_stop: 0, sys_msg_id_fail: 0,
        }
    }

    fn make_caster() -> CasterInfo {
        CasterInfo {
            object_id: 100, x: 32800, y: 32800, map_id: 4,
            heading: 0, level: 52, cur_hp: 300, cur_mp: 200,
            int_stat: 18, sp_bonus: 3, class_type: 3,
        }
    }

    fn make_target() -> TargetInfo {
        TargetInfo {
            object_id: 200, x: 32805, y: 32800, map_id: 4,
            level: 50, cur_hp: 500, max_hp: 500, cur_mp: 100,
            mr: 30, is_undead: false,
        }
    }

    #[test]
    fn test_mp_cost_reduction() {
        assert_eq!(calc_mp_cost(10, 10), 10);  // no reduction
        assert_eq!(calc_mp_cost(10, 15), 9);   // -1
        assert_eq!(calc_mp_cost(10, 20), 8);   // -2
        assert_eq!(calc_mp_cost(1, 20), 1);    // minimum 1
    }

    #[test]
    fn test_execute_attack_skill() {
        let skill = make_test_skill();
        let caster = make_caster();
        let target = make_target();
        let cd = SkillCooldowns::new();
        let effects = SkillEffects::new();

        // Run 100 times - should succeed most of the time
        let mut successes = 0;
        for _ in 0..100 {
            match execute_skill(&skill, &caster, &[target.clone()], &cd, &effects) {
                SkillResult::Success(outcome) => {
                    assert!(outcome.mp_consumed > 0);
                    assert!(!outcome.damage.is_empty());
                    let (tid, dmg) = outcome.damage[0];
                    assert_eq!(tid, 200);
                    assert!(dmg > 0, "Damage should be positive");
                    successes += 1;
                }
                SkillResult::Resisted => {} // MR can resist
                other => panic!("Unexpected result: {:?}", other),
            }
        }
        assert!(successes > 30, "Should hit at least 30% of the time, got {}", successes);
    }

    #[test]
    fn test_insufficient_mp() {
        let skill = make_test_skill();
        let mut caster = make_caster();
        caster.cur_mp = 2; // not enough
        let target = make_target();
        let cd = SkillCooldowns::new();
        let effects = SkillEffects::new();

        assert!(matches!(
            execute_skill(&skill, &caster, &[target], &cd, &effects),
            SkillResult::InsufficientMp
        ));
    }

    #[test]
    fn test_skill_cooldown() {
        let skill = make_test_skill();
        let caster = make_caster();
        let target = make_target();
        let mut cd = SkillCooldowns::new();
        cd.set_cooldown(17, 10);
        let effects = SkillEffects::new();

        assert!(matches!(
            execute_skill(&skill, &caster, &[target], &cd, &effects),
            SkillResult::OnCooldown { ticks_left: 10 }
        ));
    }

    #[test]
    fn test_buff_damage_modifiers() {
        let mut effects = SkillEffects::new();

        // No buffs → 1.0
        let mod1 = calc_buff_damage_modifier(&effects);
        assert!((mod1 - 1.0).abs() < 0.01);

        // With 暗影之牙 → +5 flat
        effects.add_effect(107, 100, 5);
        assert_eq!(calc_buff_flat_bonus(&effects), 5);

        // With 灼熱武器 → +5 more
        effects.add_effect(114, 100, 5);
        assert_eq!(calc_buff_flat_bonus(&effects), 10);
    }

    #[test]
    fn test_armor_break_multiplier() {
        let mut effects = SkillEffects::new();
        assert!((calc_armor_break_multiplier(&effects) - 1.0).abs() < 0.01);

        effects.add_effect(112, 40, 0); // 破壞盔甲
        assert!((calc_armor_break_multiplier(&effects) - 1.58).abs() < 0.01);
    }

    #[test]
    fn test_incapacitated_check() {
        let mut effects = SkillEffects::new();
        assert!(!is_incapacitated(&effects));

        effects.add_effect(120, 25, 0); // 衝擊之暈
        assert!(is_incapacitated(&effects));
    }

    #[test]
    fn test_silenced_check() {
        let mut effects = SkillEffects::new();
        assert!(!is_silenced(&effects));

        effects.add_effect(132, 80, 0); // 封印禁地
        assert!(is_silenced(&effects));
    }

    #[test]
    fn test_self_buff_no_target() {
        // Shield spell (skill 3) - self buff, no target needed
        let skill = SkillTemplate {
            skill_id: 3, name: "保護罩".into(), skill_level: 1,
            skill_number: 3, mp_consume: 8, hp_consume: 0,
            item_consume_id: 0, item_consume_count: 0,
            reuse_delay: 0, buff_duration: 1800,
            target: "self".into(), target_to: 0,
            damage_value: 2, damage_dice: 0, damage_dice_count: 0,
            probability_value: 0, attr: 0, skill_type: 0,
            is_through: false, range: 0, area: 0,
            action_id: 0, cast_gfx: 768, cast_gfx2: 0,
            sys_msg_id_happen: 0, sys_msg_id_stop: 0, sys_msg_id_fail: 0,
        };
        let caster = make_caster();
        let cd = SkillCooldowns::new();
        let effects = SkillEffects::new();

        match execute_skill(&skill, &caster, &[], &cd, &effects) {
            SkillResult::Success(outcome) => {
                assert_eq!(outcome.buffs.len(), 1);
                assert_eq!(outcome.buffs[0].0, 100); // caster's id
                assert_eq!(outcome.buffs[0].1, 3);   // skill_id = Shield
                assert_eq!(outcome.mp_consumed, 6);   // 8 - 2 (INT 18)
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }
}
