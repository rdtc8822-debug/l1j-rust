/// 投石器 (Catapult) & 傭兵 (Mercenary/Guard) siege unit system.
///
/// Based on official Lineage 1 Taiwan data from Bahamut wiki.
///
/// ## 投石器 (官方機制)
/// - 只有王族（城主方/宣戰方）可以操作
/// - 每次消耗 1 個「炸彈」（攻城時村莊雜貨店販售）
/// - 冷卻 10 秒
/// - **只對玩家和玩家攜帶的怪物（召喚/寵物）造成傷害**
/// - **不能對城門或守護塔造成傷害**
/// - 防守方：攻擊外城門外方向
/// - 攻擊方：攻擊外城門內/內城門/守護塔方向
/// - 可被破壞，攻城開始或城主交替時自動修復
///
/// ## 城堡守衛 (官方數據)
/// - 亞丁城：國王(75/8403)、親衛隊(68/11049)、親衛隊騎士(70/11513)、
///           警衛弓箭手/槍兵/戰士(66/10572)
/// - 其他城堡：親衛隊(75/8403)、親衛隊騎士(68/11049)、親衛隊牧師(70/11513)

use std::collections::HashMap;
use rand::RngExt;

// ===========================================================================
// 投石器 (Catapult) - 官方機制
// ===========================================================================

/// 投石器方向限制。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CatapultSide {
    /// 防守方投石器：只能攻擊外城門外側。
    Defender,
    /// 攻擊方投石器：可攻擊外城門內/內城門/守護塔方向。
    Attacker,
}

/// 炸彈 item ID（攻城時村莊雜貨店販售）。
pub const BOMB_ITEM_ID: i32 = 41900;

/// 投石器冷卻時間（ticks，10 秒 = 50 ticks @ 200ms/tick）。
pub const CATAPULT_RELOAD_TICKS: u32 = 50;

/// 投石器狀態。
#[derive(Debug, Clone)]
pub struct CatapultState {
    pub object_id: u32,
    pub side: CatapultSide,
    pub castle_id: i32,
    pub x: i32,
    pub y: i32,
    pub map_id: i32,
    pub cur_hp: i32,
    pub max_hp: i32,
    /// 操作中的王族 object_id（0 = 無人操作）。
    pub operator_id: u32,
    /// 裝填冷卻剩餘 ticks。
    pub reload_remaining: u32,
    /// 是否已被摧毀。
    pub destroyed: bool,
}

/// 投石器操作結果。
#[derive(Debug)]
pub enum CatapultAction {
    /// 成功發射。只對玩家/召喚造成傷害，不影響城門/塔。
    Fire { impact_x: i32, impact_y: i32, damage: i32, splash_radius: i32 },
    /// 裝填中。
    Reloading { ticks_left: u32 },
    /// 無人操作。
    NoOperator,
    /// 操作者不是王族。
    NotRoyal,
    /// 已被摧毀。
    Destroyed,
    /// 缺少炸彈。
    NoBombs,
}

impl CatapultState {
    /// 攻城開始或城主交替時建立/修復投石器。
    pub fn new(object_id: u32, castle_id: i32, side: CatapultSide, x: i32, y: i32, map_id: i32) -> Self {
        CatapultState {
            object_id, side, castle_id, x, y, map_id,
            cur_hp: 500,
            max_hp: 500,
            operator_id: 0,
            reload_remaining: 0,
            destroyed: false,
        }
    }

    /// 玩家上車操作（必須是王族）。
    pub fn mount(&mut self, player_id: u32, is_royal: bool) -> bool {
        if self.destroyed || self.operator_id != 0 || !is_royal {
            return false;
        }
        self.operator_id = player_id;
        true
    }

    /// 下車。
    pub fn dismount(&mut self) {
        self.operator_id = 0;
    }

    /// 嘗試發射（消耗 1 個炸彈，10 秒冷卻）。
    /// `has_bomb`: 呼叫方需先檢查操作者背包是否有炸彈。
    /// 官方規則：傷害只對玩家和召喚物生效。
    pub fn try_fire(&mut self, target_x: i32, target_y: i32, has_bomb: bool) -> CatapultAction {
        if self.destroyed {
            return CatapultAction::Destroyed;
        }
        if self.operator_id == 0 {
            return CatapultAction::NoOperator;
        }
        if self.reload_remaining > 0 {
            return CatapultAction::Reloading { ticks_left: self.reload_remaining };
        }
        if !has_bomb {
            return CatapultAction::NoBombs;
        }

        self.reload_remaining = CATAPULT_RELOAD_TICKS;

        // 官方投石器傷害範圍
        CatapultAction::Fire {
            impact_x: target_x,
            impact_y: target_y,
            damage: 80,        // 對玩家的傷害
            splash_radius: 3,  // 範圍 3 格
        }
    }

    /// 每 tick 更新。
    pub fn tick(&mut self) {
        if self.reload_remaining > 0 {
            self.reload_remaining -= 1;
        }
    }

    /// 受到傷害。返回 true = 摧毀。
    pub fn receive_damage(&mut self, damage: i32) -> bool {
        self.cur_hp = (self.cur_hp - damage).max(0);
        if self.cur_hp <= 0 {
            self.destroyed = true;
            self.operator_id = 0;
            true
        } else {
            false
        }
    }

    /// 自動修復（攻城開始/城主交替時）。
    pub fn repair(&mut self) {
        self.cur_hp = self.max_hp;
        self.destroyed = false;
        self.operator_id = 0;
        self.reload_remaining = 0;
    }
}

// ===========================================================================
// 城堡守衛 (Castle Guard) - 官方數據
// ===========================================================================

/// 守衛類型。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuardType {
    /// 國王 (亞丁城限定)
    King,
    /// 親衛隊
    RoyalGuard,
    /// 親衛隊騎士
    RoyalKnight,
    /// 親衛隊牧師 (非亞丁城堡)
    RoyalPriest,
    /// 警衛弓箭手 (亞丁城限定)
    GuardArcher,
    /// 警衛槍兵 (亞丁城限定)
    GuardSpearman,
    /// 警衛戰士 (亞丁城限定)
    GuardWarrior,
}

/// 官方守衛模板。
#[derive(Debug, Clone)]
pub struct GuardTemplate {
    pub guard_type: GuardType,
    pub name: &'static str,
    pub level: i32,
    pub hp: i32,
    pub is_ranged: bool,
    pub attack_range: i32,
    pub damage_min: i32,
    pub damage_max: i32,
    /// 僅在亞丁城出現。
    pub aden_only: bool,
    /// 在非亞丁城堡出現。
    pub non_aden: bool,
}

/// 官方守衛數據（巴哈姆特攻略百科）。
pub fn official_guard_templates() -> Vec<GuardTemplate> {
    vec![
        // === 亞丁城限定 ===
        GuardTemplate {
            guard_type: GuardType::King,
            name: "國王", level: 75, hp: 8_403,
            is_ranged: false, attack_range: 1,
            damage_min: 30, damage_max: 60,
            aden_only: true, non_aden: false,
        },
        GuardTemplate {
            guard_type: GuardType::GuardArcher,
            name: "警衛弓箭手", level: 66, hp: 10_572,
            is_ranged: true, attack_range: 10,
            damage_min: 20, damage_max: 45,
            aden_only: true, non_aden: false,
        },
        GuardTemplate {
            guard_type: GuardType::GuardSpearman,
            name: "警衛槍兵", level: 66, hp: 10_572,
            is_ranged: false, attack_range: 2,
            damage_min: 25, damage_max: 50,
            aden_only: true, non_aden: false,
        },
        GuardTemplate {
            guard_type: GuardType::GuardWarrior,
            name: "警衛戰士", level: 66, hp: 10_572,
            is_ranged: false, attack_range: 1,
            damage_min: 28, damage_max: 55,
            aden_only: true, non_aden: false,
        },
        // === 亞丁城的親衛隊 ===
        GuardTemplate {
            guard_type: GuardType::RoyalGuard,
            name: "親衛隊", level: 68, hp: 11_049,
            is_ranged: false, attack_range: 1,
            damage_min: 25, damage_max: 50,
            aden_only: true, non_aden: false,
        },
        GuardTemplate {
            guard_type: GuardType::RoyalKnight,
            name: "親衛隊騎士", level: 70, hp: 11_513,
            is_ranged: false, attack_range: 1,
            damage_min: 30, damage_max: 60,
            aden_only: true, non_aden: false,
        },
        // === 其他城堡（肯特/妖魔/奇岩/海音/侏儒）===
        GuardTemplate {
            guard_type: GuardType::RoyalGuard,
            name: "親衛隊", level: 75, hp: 8_403,
            is_ranged: false, attack_range: 1,
            damage_min: 30, damage_max: 55,
            aden_only: false, non_aden: true,
        },
        GuardTemplate {
            guard_type: GuardType::RoyalKnight,
            name: "親衛隊騎士", level: 68, hp: 11_049,
            is_ranged: false, attack_range: 1,
            damage_min: 25, damage_max: 50,
            aden_only: false, non_aden: true,
        },
        GuardTemplate {
            guard_type: GuardType::RoyalPriest,
            name: "親衛隊牧師", level: 70, hp: 11_513,
            is_ranged: false, attack_range: 1,
            damage_min: 20, damage_max: 40,
            aden_only: false, non_aden: true,
        },
    ]
}

/// 守衛實體。
#[derive(Debug, Clone)]
pub struct GuardState {
    pub object_id: u32,
    pub guard_type: GuardType,
    pub castle_id: i32,
    pub x: i32,
    pub y: i32,
    pub map_id: i32,
    pub heading: i32,
    pub cur_hp: i32,
    pub max_hp: i32,
    pub level: i32,
    pub target_id: u32,
    pub atk_cooldown: u32,
    pub is_alive: bool,
    pub damage_min: i32,
    pub damage_max: i32,
    pub attack_range: i32,
}

impl GuardState {
    pub fn from_template(object_id: u32, t: &GuardTemplate, castle_id: i32, x: i32, y: i32, map_id: i32) -> Self {
        GuardState {
            object_id, guard_type: t.guard_type, castle_id,
            x, y, map_id, heading: 0,
            cur_hp: t.hp, max_hp: t.hp, level: t.level,
            target_id: 0, atk_cooldown: 0, is_alive: true,
            damage_min: t.damage_min, damage_max: t.damage_max,
            attack_range: t.attack_range,
        }
    }

    pub fn tick(&mut self) {
        if self.atk_cooldown > 0 { self.atk_cooldown -= 1; }
    }

    pub fn try_attack(&mut self) -> i32 {
        if !self.is_alive || self.target_id == 0 || self.atk_cooldown > 0 { return 0; }
        self.atk_cooldown = 10; // 2 秒攻擊間隔
        rand::rng().random_range(self.damage_min..=self.damage_max)
    }

    pub fn receive_damage(&mut self, damage: i32) -> bool {
        if !self.is_alive { return true; }
        self.cur_hp = (self.cur_hp - damage).max(0);
        if self.cur_hp <= 0 { self.is_alive = false; self.target_id = 0; true }
        else { false }
    }
}

// ===========================================================================
// 黑騎士團 (官方攻城 NPC)
// ===========================================================================

/// 黑騎士團 - 攻城時城門外出現。
pub mod black_knights {
    /// 黑騎士 (Lv70, 出現 10 隻)。
    pub const BLACK_KNIGHT_LEVEL: i32 = 70;
    pub const BLACK_KNIGHT_COUNT: i32 = 10;

    /// 反王部隊賽尼斯 (Boss, Lv70, 出現 1 隻)。
    /// 掉落：對武器施法的卷軸、對盔甲施法的卷軸。
    pub const CERENIS_LEVEL: i32 = 70;

    /// 反王部隊克特 (Boss, Lv70, 出現 1 隻)。
    /// 掉落：對武器施法的卷軸、對盔甲施法的卷軸。
    pub const KEN_RAUHEL_LEVEL: i32 = 70;
}

// ===========================================================================
// 攻城 BUFF
// ===========================================================================

/// 「君主的護衛」BUFF - 攻城區域內精銳騎士以上成員自動獲得。
pub mod siege_buff {
    /// 額外攻擊力加成。
    pub const KINGS_GUARD_ATK_BONUS: i32 = 30;
    /// 需要的最低血盟階級（精銳騎士以上）。
    pub const MIN_CLAN_RANK: i32 = 6;
}

// ===========================================================================
// 攻城單位管理器
// ===========================================================================

pub struct SiegeUnitManager {
    pub catapults: HashMap<u32, CatapultState>,
    pub guards: HashMap<u32, GuardState>,
}

impl SiegeUnitManager {
    pub fn new() -> Self {
        SiegeUnitManager {
            catapults: HashMap::new(),
            guards: HashMap::new(),
        }
    }

    /// 攻城開始時修復所有投石器。
    pub fn repair_all_catapults(&mut self, castle_id: i32) {
        for cat in self.catapults.values_mut() {
            if cat.castle_id == castle_id {
                cat.repair();
            }
        }
    }

    /// 每 tick 更新。
    pub fn tick(&mut self) {
        for cat in self.catapults.values_mut() { cat.tick(); }
        for guard in self.guards.values_mut() { guard.tick(); }
    }

    /// 取得城堡存活守衛數。
    pub fn alive_guard_count(&self, castle_id: i32) -> usize {
        self.guards.values().filter(|g| g.castle_id == castle_id && g.is_alive).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catapult_official_rules() {
        let mut cat = CatapultState::new(1, 1, CatapultSide::Attacker, 100, 200, 4);

        // 非王族不能操作
        assert!(!cat.mount(100, false));
        // 王族可以操作
        assert!(cat.mount(100, true));

        // 有炸彈，發射成功
        let result = cat.try_fire(110, 210, true);
        match result {
            CatapultAction::Fire { damage, .. } => assert_eq!(damage, 80),
            _ => panic!("Expected Fire"),
        }

        // 冷卻中
        assert!(matches!(cat.try_fire(110, 210, true), CatapultAction::Reloading { .. }));

        // 等 50 ticks (10秒)
        for _ in 0..50 { cat.tick(); }
        assert!(matches!(cat.try_fire(110, 210, true), CatapultAction::Fire { .. }));

        // 沒炸彈
        for _ in 0..50 { cat.tick(); }
        assert!(matches!(cat.try_fire(110, 210, false), CatapultAction::NoBombs));
    }

    #[test]
    fn test_catapult_repair() {
        let mut cat = CatapultState::new(1, 1, CatapultSide::Defender, 100, 200, 4);
        cat.receive_damage(600);
        assert!(cat.destroyed);

        cat.repair();
        assert!(!cat.destroyed);
        assert_eq!(cat.cur_hp, cat.max_hp);
    }

    #[test]
    fn test_official_guard_hp() {
        let templates = official_guard_templates();

        // 亞丁城國王: Lv75, HP 8403
        let king = templates.iter().find(|t| t.guard_type == GuardType::King).unwrap();
        assert_eq!(king.level, 75);
        assert_eq!(king.hp, 8_403);

        // 亞丁城親衛隊: Lv68, HP 11049
        let guard = templates.iter().find(|t| t.guard_type == GuardType::RoyalGuard && t.aden_only).unwrap();
        assert_eq!(guard.level, 68);
        assert_eq!(guard.hp, 11_049);

        // 亞丁城親衛隊騎士: Lv70, HP 11513
        let knight = templates.iter().find(|t| t.guard_type == GuardType::RoyalKnight && t.aden_only).unwrap();
        assert_eq!(knight.level, 70);
        assert_eq!(knight.hp, 11_513);

        // 亞丁城警衛弓箭手: Lv66, HP 10572
        let archer = templates.iter().find(|t| t.guard_type == GuardType::GuardArcher).unwrap();
        assert_eq!(archer.level, 66);
        assert_eq!(archer.hp, 10_572);

        // 其他城堡親衛隊: Lv75, HP 8403
        let other_guard = templates.iter().find(|t| t.guard_type == GuardType::RoyalGuard && t.non_aden).unwrap();
        assert_eq!(other_guard.level, 75);
        assert_eq!(other_guard.hp, 8_403);

        // 其他城堡牧師: Lv70, HP 11513
        let priest = templates.iter().find(|t| t.guard_type == GuardType::RoyalPriest).unwrap();
        assert_eq!(priest.level, 70);
        assert_eq!(priest.hp, 11_513);
    }

    #[test]
    fn test_guard_combat() {
        let templates = official_guard_templates();
        let knight_t = templates.iter().find(|t| t.guard_type == GuardType::RoyalKnight && t.aden_only).unwrap();

        let mut guard = GuardState::from_template(1, knight_t, 7, 100, 200, 4);
        assert_eq!(guard.max_hp, 11_513);
        guard.target_id = 999;

        let dmg = guard.try_attack();
        assert!(dmg >= 30 && dmg <= 60);

        // 攻擊冷卻
        assert_eq!(guard.try_attack(), 0);
    }

    #[test]
    fn test_siege_buff_constants() {
        assert_eq!(siege_buff::KINGS_GUARD_ATK_BONUS, 30);
        assert_eq!(siege_buff::MIN_CLAN_RANK, 6);
    }

    #[test]
    fn test_black_knight_constants() {
        assert_eq!(black_knights::BLACK_KNIGHT_COUNT, 10);
        assert_eq!(black_knights::CERENIS_LEVEL, 70);
    }
}
