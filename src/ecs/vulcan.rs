/// 火神的工匠（赫菲斯托）系統 - 基於官方數據。
///
/// NPC 位置：奇岩村莊 (33456, 32776)
/// 功能：熔煉裝備 → 火神結晶體、製作武器/防具
///
/// 資料來源：天堂官方活動頁面、17173 天堂攻略
use rand::RngExt;

/// 火神結晶體 item ID。
pub const VULCAN_CRYSTAL_ID: i32 = 41246;
/// 火神契約 item ID。
pub const VULCAN_CONTRACT_ID: i32 = 41247;
/// 火神之槌 item ID（提升製作成功率）。
pub const VULCAN_HAMMER_ID: i32 = 41248;

/// 火神工匠 NPC 位置。
pub const VULCAN_NPC_X: i32 = 33456;
pub const VULCAN_NPC_Y: i32 = 32776;
pub const VULCAN_NPC_MAP: i32 = 4;

// ===========================================================================
// 熔煉系統：裝備 → 火神結晶體
// ===========================================================================

/// 防具熔煉表（官方數據）。
/// (物品名, 最低強化等級, 各等級獲得結晶體數量)
#[derive(Debug, Clone)]
pub struct SmeltEntry {
    pub item_name: &'static str,
    pub item_id: i32,
    pub is_weapon: bool,
    pub min_enchant: i32,
    /// (強化等級, 獲得結晶體數量)
    pub crystal_by_enchant: &'static [(i32, i32)],
}

/// 官方防具熔煉數據。
pub fn armor_smelt_table() -> Vec<SmeltEntry> {
    vec![
        SmeltEntry {
            item_name: "金屬盔甲", item_id: 20011, is_weapon: false, min_enchant: 4,
            crystal_by_enchant: &[(4, 1), (5, 3), (6, 7), (7, 13), (8, 23), (9, 39), (10, 65)],
        },
        SmeltEntry {
            item_name: "力量手套", item_id: 20164, is_weapon: false, min_enchant: 4,
            crystal_by_enchant: &[(4, 9), (5, 25), (6, 55), (7, 107), (8, 189), (9, 353), (10, 583)],
        },
        SmeltEntry {
            item_name: "法師之帽", item_id: 20013, is_weapon: false, min_enchant: 4,
            crystal_by_enchant: &[(4, 8), (5, 22), (6, 49), (7, 95), (8, 168), (9, 311), (10, 518)],
        },
    ]
}

/// 官方武器熔煉數據。
pub fn weapon_smelt_table() -> Vec<SmeltEntry> {
    vec![
        SmeltEntry {
            item_name: "長劍", item_id: 4, is_weapon: true, min_enchant: 6,
            crystal_by_enchant: &[(6, 2), (7, 5), (8, 9), (9, 16), (10, 27), (11, 37), (12, 48)],
        },
        SmeltEntry {
            item_name: "瑟魯基之劍", item_id: 54, is_weapon: true, min_enchant: 6,
            crystal_by_enchant: &[(6, 10), (7, 22), (8, 42), (9, 72), (10, 120), (11, 175), (12, 240)],
        },
        SmeltEntry {
            item_name: "大馬士革雙刀", item_id: 64, is_weapon: true, min_enchant: 6,
            crystal_by_enchant: &[(6, 13), (7, 28), (8, 54), (9, 93), (10, 155), (11, 225), (12, 312)],
        },
    ]
}

/// 計算熔煉獲得的火神結晶體數量。
/// 返回 None 表示該裝備/強化等級不可熔煉。
pub fn calc_smelt_crystals(item_id: i32, enchant_level: i32) -> Option<i32> {
    let all_tables: Vec<SmeltEntry> = armor_smelt_table()
        .into_iter()
        .chain(weapon_smelt_table())
        .collect();

    for entry in &all_tables {
        if entry.item_id == item_id {
            if enchant_level < entry.min_enchant {
                return None;
            }
            for &(enc, crystals) in entry.crystal_by_enchant {
                if enc == enchant_level {
                    return Some(crystals);
                }
            }
            return None; // 超過表中最高等級
        }
    }
    None // 不在熔煉表中
}

// ===========================================================================
// 製作系統：火神契約 + 火神結晶體 → 武器/防具
// ===========================================================================

/// 製作配方。
#[derive(Debug, Clone)]
pub struct CraftRecipe {
    pub result_item_name: &'static str,
    pub result_item_id: i32,
    pub contract_cost: i32,      // 火神契約數量
    pub crystal_cost: i32,       // 火神結晶體數量
    /// 基礎成功率（%）。使用火神之槌可提升。
    pub base_success_rate: i32,
    /// 火神之槌提升的額外成功率（%）。
    pub hammer_bonus_rate: i32,
}

/// 官方製作配方表。
pub fn craft_recipes() -> Vec<CraftRecipe> {
    vec![
        // 武器
        CraftRecipe {
            result_item_name: "武官之刃", result_item_id: 80,
            contract_cost: 8, crystal_cost: 40,
            base_success_rate: 80, hammer_bonus_rate: 10,
        },
        CraftRecipe {
            result_item_name: "黑暗雙刀", result_item_id: 81,
            contract_cost: 10, crystal_cost: 40,
            base_success_rate: 75, hammer_bonus_rate: 10,
        },
        CraftRecipe {
            result_item_name: "克特之劍", result_item_id: 82,
            contract_cost: 9, crystal_cost: 75,
            base_success_rate: 70, hammer_bonus_rate: 10,
        },
        CraftRecipe {
            result_item_name: "宙斯巨劍", result_item_id: 83,
            contract_cost: 20, crystal_cost: 80,
            base_success_rate: 60, hammer_bonus_rate: 15,
        },
        CraftRecipe {
            result_item_name: "瑪那魔杖", result_item_id: 84,
            contract_cost: 40, crystal_cost: 60,
            base_success_rate: 55, hammer_bonus_rate: 15,
        },
        // 防具
        CraftRecipe {
            result_item_name: "蚩尤鎧甲", result_item_id: 20200,
            contract_cost: 5, crystal_cost: 100,
            base_success_rate: 65, hammer_bonus_rate: 10,
        },
        CraftRecipe {
            result_item_name: "黑長者長袍", result_item_id: 20201,
            contract_cost: 5, crystal_cost: 100,
            base_success_rate: 65, hammer_bonus_rate: 10,
        },
    ]
}

/// 嘗試製作結果。
#[derive(Debug, PartialEq)]
pub enum CraftResult {
    /// 製作成功，返回產出物品 ID。
    Success(i32),
    /// 製作失敗，材料消耗。
    Failure,
    /// 材料不足。
    InsufficientMaterials,
    /// 配方不存在。
    RecipeNotFound,
}

/// 執行製作判定。
pub fn try_craft(
    recipe_item_id: i32,
    has_contracts: i32,
    has_crystals: i32,
    has_hammer: bool,
) -> CraftResult {
    let recipes = craft_recipes();
    let recipe = match recipes.iter().find(|r| r.result_item_id == recipe_item_id) {
        Some(r) => r,
        None => return CraftResult::RecipeNotFound,
    };

    if has_contracts < recipe.contract_cost || has_crystals < recipe.crystal_cost {
        return CraftResult::InsufficientMaterials;
    }

    let success_rate = if has_hammer {
        recipe.base_success_rate + recipe.hammer_bonus_rate
    } else {
        recipe.base_success_rate
    };

    let roll = rand::rng().random_range(1..=100);

    if roll <= success_rate {
        CraftResult::Success(recipe.result_item_id)
    } else {
        CraftResult::Failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smelt_gold_armor() {
        // 金屬盔甲 +4 = 1 結晶
        assert_eq!(calc_smelt_crystals(20011, 4), Some(1));
        // 金屬盔甲 +10 = 65 結晶
        assert_eq!(calc_smelt_crystals(20011, 10), Some(65));
        // 金屬盔甲 +3 不可熔煉
        assert_eq!(calc_smelt_crystals(20011, 3), None);
    }

    #[test]
    fn test_smelt_power_glove() {
        // 力量手套 +10 = 583 結晶
        assert_eq!(calc_smelt_crystals(20164, 10), Some(583));
        // 力量手套 +7 = 107 結晶
        assert_eq!(calc_smelt_crystals(20164, 7), Some(107));
    }

    #[test]
    fn test_smelt_damascus() {
        // 大馬士革雙刀 +12 = 312 結晶
        assert_eq!(calc_smelt_crystals(64, 12), Some(312));
        // 大馬士革雙刀 +6 = 13 結晶
        assert_eq!(calc_smelt_crystals(64, 6), Some(13));
    }

    #[test]
    fn test_smelt_unknown_item() {
        assert_eq!(calc_smelt_crystals(99999, 10), None);
    }

    #[test]
    fn test_craft_insufficient_materials() {
        let result = try_craft(80, 3, 20, false); // 武官之刃需要 8 契約 40 結晶
        assert_eq!(result, CraftResult::InsufficientMaterials);
    }

    #[test]
    fn test_craft_recipe_not_found() {
        let result = try_craft(99999, 100, 100, false);
        assert_eq!(result, CraftResult::RecipeNotFound);
    }

    #[test]
    fn test_craft_with_materials() {
        // 用大量嘗試確認成功率合理
        let mut successes = 0;
        for _ in 0..1000 {
            match try_craft(80, 100, 100, false) {
                CraftResult::Success(_) => successes += 1,
                CraftResult::Failure => {}
                _ => panic!("Unexpected result"),
            }
        }
        // 武官之刃基礎成功率 80%，1000 次中約 750-850 次成功
        assert!(successes > 700 && successes < 900,
            "Success count {} outside expected range for 80% rate", successes);
    }

    #[test]
    fn test_craft_hammer_bonus() {
        let mut with_hammer = 0;
        let mut without_hammer = 0;
        for _ in 0..1000 {
            if let CraftResult::Success(_) = try_craft(83, 100, 100, true) { with_hammer += 1; }
            if let CraftResult::Success(_) = try_craft(83, 100, 100, false) { without_hammer += 1; }
        }
        // 宙斯巨劍：基礎 60%，火神之槌 +15% = 75%
        // 有槌應該比沒槌高
        assert!(with_hammer > without_hammer,
            "Hammer bonus not working: with={}, without={}", with_hammer, without_hammer);
    }
}
