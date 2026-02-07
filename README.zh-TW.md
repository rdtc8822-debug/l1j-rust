# L1J-Rust

使用 Rust 語言編寫的《天堂 1》（Lineage 1，3.80c 台灣版本）遊戲伺服器。

這是對 L1J-TW Java 伺服器的完整重寫，旨在追求極致效能 — 能夠在不延遲（Lag）的情況下同時處理畫面上超過 10,000 個 NPC。

<p align="right">
  <a href="README.md">English</a> |
  <b>繁體中文</b> |
  <a href="README.zh-CN.md">简体中文</a>
</p>

## 特色

### 網路系統
- Tokio 非同步 TCP 監聽器（Non-blocking I/O）
- XOR 加密演算法（從 Java 版 `Cipher.java` 1:1 移植）
- 封包框架編解碼器（Packet frame codec，2 位元組 LE 長度標頭）
- 完整支援 3.80c 台灣版本協定 — 支援超過 250 個操作碼（Opcodes）
- 支援 Big5/MS950 編碼以正確顯示中文字元

### 登入系統
- 帳號驗證（SHA-1 + Base64，與 Java 版一致）
- 初次登入時自動建立帳號
- 帳號在線/離線狀態追蹤
- 斷線清理（自動登出）

### 角色系統
- 支援所有 7 個職業的角色創建（王族、騎士、妖精、法師、黑暗妖精、龍騎士、幻術師）
- 官方各職業初始能力值與 HP/MP 成長數據
- 角色列表顯示
- 角色選擇與進入遊戲流程（包含 17 個以上的初始化封包）
- 位置追蹤並在斷線時儲存座標

### 多人遊戲
- 共享世界狀態 — 玩家之間可以互相看見
- 即時移動廣播（S_MOVECHARPACKET）
- 鄰近區域廣播聊天系統
- 玩家外觀封包（S_CHARPACK）
- 玩家斷線時自動從其他玩家畫面移除角色

### 世界系統
- 基於網格的空間劃分（32x32 圖塊區域）
- O(k) 可見度查詢，取代 O(n) 的暴力搜尋
- 支援 V1（文字）與 V2（二進位壓縮）地圖格式
- 地圖通行性、安全區、戰鬥區判定

### 遊戲引擎
- 基於 Tick 的 AI 引擎（單一迴圈處理所有 NPC）
- NPC 睡眠優化（附近沒有玩家時跳過 AI 運算）
- 戰鬥計算（命中/閃避、傷害、暴擊）
- 從資料庫載入 NPC 生成系統

### 攻城系統 (官方數據)
- 支援 8 座城堡，包含戰爭區域、塔樓位置、內城地圖
- 戰爭狀態機（宣戰 → 進行中 → 勝利/超時）
- 城門系統（6 種損壞狀態與摧毀機制）
- 守護塔（4 種裂痕狀態，包含亞丁副塔邏輯）
- 塔樓毀壞後掉落皇冠
- 投石機系統（官方規則：僅對玩家造成傷害，10 秒冷卻時間）
- 官方 HP 數值的城門守軍（根據巴哈姆特維基數據）
- 攻城增益：近衛兵的鼓舞（攻擊力 +30）
- 城門口的黑騎士 NPC

### 技能系統
- 完整 7 職業技能樹與官方數據
  - **黑暗妖精**：16 種技能（破壞盔甲 +58% 傷害、燃燒鬥志 34% 機率 1.5 倍等）
  - **騎士**：衝擊之暈、反擊屏障 35% 觸發、增幅防禦
  - **龍騎士**：屠宰者（150HP 汲取）、奪命之雷、龍之護鎧
  - **幻術師**：幻覺：化身、疼痛的歡愉、混亂、石化
  - **王族**：烈炎武器、閃耀盾牌、呼喚盟友、勇猛意志
  - **妖精**：三重矢、烈炎術、封印禁地、召喚精靈
- 支援 MP/智力 消耗減免的技能執行引擎
- 基於 Tick 的增益/減益（Buff/Debuff）過期系統
- 技能冷卻時間追蹤
- 魔法防禦（MR）計算
- 增益傷害修正與戰鬥系統整合

### 物品系統
- 從 MySQL 載入物品模板（一般物品、武器、防具）
- 背包管理（可疊加/不可疊加、增加/移除/檢查）
- 裝備欄位系統
- 實作 S_AddItem, S_DeleteItem, S_InvList, S_ItemStatus 封包

### 赫發斯特斯（Vulcan）鍛造系統 (官方數據)
- 裝備熔煉 → 獲得赫發斯特斯原石（官方數量數據）
- 使用鍛造契約與原石進行制作
- 使用赫發斯特斯之錘可獲得成功率加成
- 官方配方數據（武官之刃、克特之劍、宙斯巨劍等）

### 傳送系統
- 從 MySQL 載入地監入口列表
- 記憶座標（書籤）增刪查改功能
- 包含地圖切換與特效動畫的傳送動作
- C_ENTERPORTAL, C_BOOKMARK, C_BOOKMARKDELETE 處理程式

### 血盟系統
- 血盟 CRUD（clan_data 與 clan_members 資料表）
- 10 個職位等級（從一般成員到守護者、王族）
- 創建/加入/離開/逐出處理程式
- 宣戰封包
- 城主皇冠顯示
- 支援血盟徽章

## 效能表現

在單機模式下進行基準測試（Release 編譯）：

| 指標 | 結果 |
|--------|--------|
| 生成 10,000 個 NPC | 4.49 毫秒 |
| 10,000 個 NPC Tick（AI + 移動） | 平均 0.478 毫秒 / 最高 0.735 毫秒 |
| Tick 預算（200 毫秒）使用率 | 0.37% |
| 理論 NPC 承載上限 | 約 270,000 個 |
| 10,000 次可見度查詢 | 58.8 毫秒（每次 5.88 微秒） |

## 測試結果

102 個單元測試 + 2 個壓力測試，全數通過。

## 需求

- Rust 1.70+ (已在 1.93.0 上測試)
- MySQL 8.0+
- 天堂 1 客戶端 (3.80c 台灣版本)

## 快速開始

1. 安裝 MySQL 並建立資料庫：
```sql
CREATE DATABASE l1jdb CHARACTER SET utf8;
```

2. 匯入 L1J-TW 資料庫資料表（來自原始 Java 伺服器的 `db/` 資料夾）。

3. 編輯 `config/server.example.toml 改成 server.toml`：
```toml
[database]
url = "mysql://root:YOUR_PASSWORD@localhost:3306/l1jdb"
```

4. 建置並執行：
```bash
cd l1j-rust
cargo run --release
```

5. 設定你的 3.80c 客戶端登入工具：
   - IP: `127.0.0.1`
   - Port: `7000`
   - Version: `TW13081901`

6. 啟動客戶端，建立帳號（初次登入自動建立），創建角色並開始遊戲。

## 專案結構

```
l1j-rust/
  Cargo.toml                         # 相依套件設定
  config/server.toml                  # 伺服器設定檔
  src/
    main.rs                           # 程式進入點
    lib.rs                            # 模組宣告
    config.rs                         # TOML 設定載入
    network/
      cipher.rs                       # XOR 封包加密
      codec.rs                        # 封包框架編解碼器
      listener.rs                     # TCP 監聽器
      session.rs                      # 客戶端連線階段 (Session) 處理
      shared_state.rs                 # 共享世界狀態 (多人連線用)
    protocol/
      encoding.rs                     # Big5/MS950 編碼處理
      opcodes.rs                      # 3.80c 操作碼 (250+)
      packet.rs                       # PacketBuilder + PacketReader
      client/                         # 客戶端封包解析器 (10 個模組)
      server/                         # 伺服器封包建構器 (15 個模組)
    db/
      pool.rs                         # MySQL 連線池
      account.rs                      # 帳號驗證 (SHA-1)
      character.rs                    # 角色 CRUD
      char_create.rs                  # 角色創建
      clan.rs                         # 血盟 CRUD
    data/
      npc_table.rs                    # NPC 模板載入
      item_table.rs                   # 物品模板載入
      skill_table.rs                  # 技能模板載入
      spawn_table.rs                  # 生怪點載入
      dungeon_table.rs                # 傳送點/地監載入
      bookmark_table.rs               # 傳送記憶座標載入
    world/
      grid.rs                         # 32x32 空間劃分
      map_data.rs                     # V1/V2 地圖格式
    ecs/
      game_engine.rs                  # 基於 Tick 的 AI 引擎
      combat.rs                       # 傷害計算
      skill_executor.rs               # 技能執行引擎
      siege.rs                        # 城堡攻城系統
      siege_units.rs                  # 投石機 + 守衛 (官方數據)
      class_skills.rs                 # 所有職業技能樹
      darkelf_skills.rs               # 黑暗妖精技能 (官方數據)
      vulcan.rs                       # 赫發斯特斯鍛造系統
      components/                     # ECS 組件 (9 個模組)
  tests/
    stress_test.rs                    # 10,000 NPC 壓力測試
```

## 授權

本專案僅供教育與研究用途。

## 致謝

- 參考原始 L1J-TW 3.80c Java 伺服器
- 遊戲數據來自巴哈姆特維基 (Bahamut wiki)、LoA 3.63 以及官方來源