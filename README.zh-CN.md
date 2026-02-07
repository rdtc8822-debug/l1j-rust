# L1J-Rust

使用 Rust 语言编写的《天堂 1》（Lineage 1，3.80c 台湾版本）游戏服务端。

这是对 L1J-TW Java 服务端的完整重写，旨在追求极致性能 — 能够在不延迟（Lag）的情况下同时处理屏幕上超过 10,000 个 NPC。

<p align="right">
  <a href="README.md">English</a> |
  <a href="README.zh-TW.md">繁體中文</a> |
  <b>简体中文</b>
</p>

## 特色

### 网络系统
- Tokio 异步 TCP 监听器（Non-blocking I/O）
- XOR 加密算法（从 Java 版 `Cipher.java` 1:1 移植）
- 数据包框架编解码器（Packet frame codec，2 字节 LE 长度标头）
- 完整支持 3.80c 台湾版本协议 — 支持超过 250 个操作码（Opcodes）
- 支持 Big5/MS950 编码以正确显示中文字符

### 登录系统
- 账号验证（SHA-1 + Base64，与 Java 版一致）
- 初次登录时自动创建账号
- 账号在线/离线状态追踪
- 断线清理（自动登出）

### 角色系统
- 支持所有 7 个职业的角色创建（王族、骑士、妖精、法师、黑暗妖精、龙骑士、幻术师）
- 官方各职业初始属性点与 HP/MP 成长数据
- 角色列表显示
- 角色选择与进入游戏流程（包含 17 个以上的初始化数据包）
- 位置追踪并在断线时保存坐标

### 多人游戏
- 共享世界状态 — 玩家之间可以互相看见
- 实时移动广播（S_MOVECHARPACKET）
- 邻近区域广播聊天系统
- 玩家外观数据包（S_CHARPACK）
- 玩家断线时自动从其他玩家画面移除角色

### 世界系统
- 基于网格的空间划分（32x32 图块区域）
- O(k) 可见度查询，取代 O(n) 的暴力搜索
- 支持 V1（文本）与 V2（二进制压缩）地图格式
- 地图通行性、安全区、战斗区判定

### 游戏引擎
- 基于 Tick 的 AI 引擎（单一循环处理所有 NPC）
- NPC 睡眠优化（附近没有玩家时跳过 AI 运算）
- 战斗计算（命中/闪避、伤害、暴击）
- 从数据库加载 NPC 生成系统

### 攻城系统 (官方数据)
- 支持 8 座城堡，包含战争区域、塔楼位置、内城地图
- 战争状态机（宣战 → 进行中 → 胜利/超时）
- 城门系统（6 种损坏状态与摧毁机制）
- 守护塔（4 种裂痕状态，包含亚丁副塔逻辑）
- 塔楼毁坏后掉落皇冠
- 投石机系统（官方规则：仅对玩家造成伤害，10 秒冷却时间）
- 官方 HP 数值的城门守军（根据巴哈姆特维基数据）
- 攻城增益：近卫兵的鼓舞（攻击力 +30）
- 城门口的黑骑士 NPC

### 技能系统
- 完整 7 职业技能树与官方数据
  - **黑暗妖精**：16 种技能（破坏盔甲 +58% 伤害、燃烧斗志 34% 几率 1.5 倍等）
  - **骑士**：冲击之晕、反击屏障 35% 触发、增幅防御
  - **龙骑士**：屠宰者（150HP 汲取）、夺命之雷、龙之护铠
  - **幻术师**：幻觉：化身、疼痛的欢愉、混乱、石化
  - **王族**：烈炎武器、闪耀盾牌、呼唤盟友、勇猛意志
  - **妖精**：三重矢、烈炎术、封印禁地、召唤精灵
- 支持 MP/智力 消耗减免的技能执行引擎
- 基于 Tick 的增益/减益（Buff/Debuff）过期系统
- 技能冷却时间追踪
- 魔法防御（MR）计算
- 增益伤害修正与战斗系统整合

### 物品系统
- 从 MySQL 加载物品模板（一般物品、武器、防具）
- 背包管理（可叠加/不可叠加、增加/移除/检查）
- 装备栏位系统
- 实现 S_AddItem, S_DeleteItem, S_InvList, S_ItemStatus 数据包

### 赫发斯特斯（Vulcan）锻造系统 (官方数据)
- 装备熔炼 → 获得赫发斯特斯原石（官方数量数据）
- 使用锻造契约与原石进行制作
- 使用赫发斯特斯之锤可获得成功率加成
- 官方配方数据（武官之刃、克特之剑、宙斯巨剑等）

### 传送系统
- 从 MySQL 加载地牢入口列表
- 记忆坐标（书签）增删改查功能
- 包含地图切换与特效动画的传送动作
- C_ENTERPORTAL, C_BOOKMARK, C_BOOKMARKDELETE 处理程序

### 血盟系统
- 血盟 CRUD（clan_data 与 clan_members 数据表）
- 10 个职位等级（从一般成员到守护者、王族）
- 创建/加入/离开/踢出处理程序
- 宣战数据包
- 城主皇冠显示
- 支持血盟徽章

## 性能表现

在单机模式下进行基准测试（Release 编译）：

| 指标 | 结果 |
|--------|--------|
| 生成 10,000 个 NPC | 4.49 毫秒 |
| 10,000 个 NPC Tick（AI + 移动） | 平均 0.478 毫秒 / 最高 0.735 毫秒 |
| Tick 预算（200 毫秒）使用率 | 0.37% |
| 理论 NPC 承载上限 | 约 270,000 个 |
| 10,000 次可见度查询 | 58.8 毫秒（每次 5.88 微秒） |

## 测试结果

102 个单元测试 + 2 个压力测试，全部通过。

## 需求

- Rust 1.70+ (已在 1.93.0 上测试)
- MySQL 8.0+
- 天堂 1 客户端 (3.80c 台湾版本)

## 快速开始

1. 安装 MySQL 并创建数据库：
```sql
CREATE DATABASE l1jdb CHARACTER SET utf8;
```

2. 导入 L1J-TW 数据库表（来自原始 Java 服务端的 `db/` 文件夹）。

3. 编辑 `config/server.example.toml 改成 server.toml`：
```toml
[database]
url = "mysql://root:YOUR_PASSWORD@localhost:3306/l1jdb"
```

4. 构建并运行：
```bash
cd l1j-rust
cargo run --release
```

5. 配置你的 3.80c 客户端登录工具：
   - IP: `127.0.0.1`
   - Port: `7000`
   - Version: `TW13081901`

6. 启动客户端，创建账号（初次登录自动创建），创建角色并开始游戏。

## 项目结构

```
l1j-rust/
  Cargo.toml                         # 依赖配置
  config/server.toml                  # 服务端配置文件
  src/
    main.rs                           # 程序入口点
    lib.rs                            # 模块声明
    config.rs                         # TOML 配置加载
    network/
      cipher.rs                       # XOR 数据包加密
      codec.rs                        # 数据包框架编解码器
      listener.rs                     # TCP 监听器
      session.rs                      # 客户端会话 (Session) 处理
      shared_state.rs                 # 共享世界状态 (多人联机用)
    protocol/
      encoding.rs                     # Big5/MS950 编码处理
      opcodes.rs                      # 3.80c 操作码 (250+)
      packet.rs                       # PacketBuilder + PacketReader
      client/                         # 客户端数据包解析器 (10 个模块)
      server/                         # 服务端数据包构建器 (15 个模块)
    db/
      pool.rs                         # MySQL 连接池
      account.rs                      # 账号验证 (SHA-1)
      character.rs                    # 角色 CRUD
      char_create.rs                  # 角色创建
      clan.rs                         # 血盟 CRUD
    data/
      npc_table.rs                    # NPC 模板加载
      item_table.rs                   # 物品模板加载
      skill_table.rs                  # 技能模板加载
      spawn_table.rs                  # 刷怪点加载
      dungeon_table.rs                # 传送点/地牢加载
      bookmark_table.rs               # 传送记忆坐标加载
    world/
      grid.rs                         # 32x32 空间划分
      map_data.rs                     # V1/V2 地图格式
    ecs/
      game_engine.rs                  # 基于 Tick 的 AI 引擎
      combat.rs                       # 伤害计算
      skill_executor.rs               # 技能执行引擎
      siege.rs                        # 城堡攻城系统
      siege_units.rs                  # 投石机 + 守卫 (官方数据)
      class_skills.rs                 # 所有职业技能树
      darkelf_skills.rs               # 黑暗妖精技能 (官方数据)
      vulcan.rs                       # 赫发斯特斯锻造系统
      components/                     # ECS 组件 (9 个模块)
  tests/
    stress_test.rs                    # 10,000 NPC 压力测试
```

## 授权

本项目仅供教育与研究用途。

## 致谢

- 参考原始 L1J-TW 3.80c Java 服务端
- 游戏数据来自巴哈姆特维基 (Bahamut wiki)、LoA 3.63 以及官方来源