# L1J-Rust

Lineage 1 (3.80c Taiwan) game server written in Rust.

A complete rewrite of the L1J-TW Java server, designed for extreme performance — capable of handling 10,000+ NPCs on screen without lag.

<p align="right">
  <b>English</b> |
  <a href="README.zh-TW.md">繁體中文</a> |
  <a href="README.zh-CN.md">简体中文</a>
</p>

## Features

### Networking
- Tokio async TCP listener (non-blocking I/O)
- XOR Cipher encryption (1:1 port from Java `Cipher.java`)
- Packet frame codec (2-byte LE length header)
- Full 3.80c Taiwan protocol — 250+ opcodes supported
- Big5/MS950 encoding for Chinese character display

### Login System
- Account authentication (SHA-1 + Base64, matching Java)
- Auto-create accounts on first login
- Account online/offline status tracking
- Disconnect cleanup (auto-logout)

### Character System
- Character creation with all 7 classes (Prince, Knight, Elf, Mage, DarkElf, DragonKnight, Illusionist)
- Official base stats and HP/MP per class
- Character list display
- Character selection and game entry (17+ init packets)
- Position tracking and save on disconnect

### Multiplayer
- Shared world state — players can see each other
- Real-time movement broadcasting (S_MOVECHARPACKET)
- Chat system with nearby broadcast
- Player appearance packets (S_CHARPACK)
- Player disconnect removes character from other players' screens

### World System
- Grid-based spatial partitioning (32x32 tile regions)
- O(k) visibility queries instead of O(n) brute force
- V1 (text) and V2 (binary compressed) map format support
- Map passability, safety zones, combat zones

### Game Engine
- Tick-based AI engine (single loop processes all NPCs)
- NPC sleep optimization (skip AI when no players nearby)
- Combat calculation (hit/miss, damage, critical)
- NPC spawn system with database loading

### Siege System (Official Data)
- 8 castles with war areas, tower locations, inner maps
- War state machine (declare → active → victory/timeout)
- Castle doors (6 damage states + destruction)
- Guardian towers (4 crack states, Aden sub-tower logic)
- Crown spawning when tower destroyed
- Catapult system (official rules: only damages players, 10s cooldown)
- Castle guards with official HP values (Bahamut wiki data)
- Siege buff: King's Guard (ATK+30)
- Black Knight NPCs at castle gates

### Skill System
- All 7 class skill trees with official data
  - Dark Elf: 16 skills (Armor Break +58% damage, Burning Spirit 34%×1.5, etc.)
  - Knight: Stun Bash, Counter Barrier 35%, Amplified Defense
  - Dragon Knight: Slaughter (150HP drain), Thunder Grab, Dragon Skin
  - Illusionist: Illusion Avatar, Pain of Joy, Confusion, Petrify
  - Royal: Blazing Weapon, Shining Shield, Call Clan, Brave Will
  - Elf: Triple Arrow, Spirit Fire, Seal Area, Summon Elemental
- Skill execution engine with MP/INT cost reduction
- Buff/debuff system with tick-based expiry
- Skill cooldown tracking
- Magic resistance calculation
- Buff damage modifiers integrated with combat

### Item System
- Item templates (EtcItem, Weapon, Armor) loaded from MySQL
- Inventory management (stackable/non-stackable, add/remove/check)
- Equipment slot system
- S_AddItem, S_DeleteItem, S_InvList, S_ItemStatus packets

### Vulcan Forge System (Official Data)
- Equipment smelting → Vulcan Crystals (official crystal amounts)
- Crafting with Vulcan Contract + Crystals
- Vulcan Hammer success rate bonus
- Official recipe data (武官之刃, 克特之劍, 宙斯巨劍, etc.)

### Teleport System
- Dungeon portal table loaded from MySQL
- Bookmark (記憶座標) CRUD
- Teleport action with map change + effect animation
- C_ENTERPORTAL, C_BOOKMARK, C_BOOKMARKDELETE handlers

### Clan System
- Clan CRUD (clan_data + clan_members tables)
- 10 rank levels (Public → Guardian → Prince)
- Create/join/leave/kick handlers
- War declaration packets
- Castle master crown display
- Clan emblem support

## Performance

Benchmarked on a single machine (Release mode):

| Metric | Result |
|--------|--------|
| Spawn 10,000 NPCs | 4.49ms |
| Tick 10,000 NPCs (AI + movement) | avg 0.478ms / max 0.735ms |
| Tick budget (200ms) usage | 0.37% |
| Theoretical NPC limit | ~270,000 |
| 10,000 visibility queries | 58.8ms (5.88μs each) |

## Test Results

102 unit tests + 2 stress tests, all passing.

## Requirements

- Rust 1.70+ (tested on 1.93.0)
- MySQL 8.0+
- Lineage 1 client (3.80c Taiwan version)

## Quick Start

1. Install MySQL and create a database:
```sql
CREATE DATABASE l1jdb CHARACTER SET utf8;
```

2. Import the L1J-TW database tables (from the original Java server's `db/` folder).

3. Edit `config/server.example.toml change server.toml`:
```toml
[database]
url = "mysql://root:youpassword@localhost:3306/l1jdb"
```

4. Build and run:
```bash
cd l1j-rust
cargo run --release
```

5. Configure your 3.80c client login tool:
   - IP: `127.0.0.1`
   - Port: `7000`
   - Version: `TW13081901`

6. Launch client, create account (auto-created on first login), create character, and play.

## Project Structure

```
l1j-rust/
  Cargo.toml                         # Dependencies
  config/server.toml                  # Server configuration
  src/
    main.rs                           # Entry point
    lib.rs                            # Module declarations
    config.rs                         # TOML config loading
    network/
      cipher.rs                       # XOR packet encryption
      codec.rs                        # Packet frame codec
      listener.rs                     # TCP listener
      session.rs                      # Client session handler
      shared_state.rs                 # Shared world (multiplayer)
    protocol/
      encoding.rs                     # Big5/MS950 encoding
      opcodes.rs                      # 3.80c opcodes (250+)
      packet.rs                       # PacketBuilder + PacketReader
      client/                         # Client packet parsers (10 modules)
      server/                         # Server packet builders (15 modules)
    db/
      pool.rs                         # MySQL connection pool
      account.rs                      # Account auth (SHA-1)
      character.rs                    # Character CRUD
      char_create.rs                  # Character creation
      clan.rs                         # Clan CRUD
    data/
      npc_table.rs                    # NPC template loading
      item_table.rs                   # Item template loading
      skill_table.rs                  # Skill template loading
      spawn_table.rs                  # Spawn point loading
      dungeon_table.rs                # Portal/dungeon loading
      bookmark_table.rs               # Teleport bookmark loading
    world/
      grid.rs                         # 32x32 spatial partitioning
      map_data.rs                     # V1/V2 map format
    ecs/
      game_engine.rs                  # Tick-based AI engine
      combat.rs                       # Damage calculation
      skill_executor.rs               # Skill execution engine
      siege.rs                        # Castle siege system
      siege_units.rs                  # Catapults + guards (official data)
      class_skills.rs                 # All class skill trees
      darkelf_skills.rs               # Dark Elf skills (official data)
      vulcan.rs                       # Vulcan Forge crafting
      components/                     # ECS components (9 modules)
  tests/
    stress_test.rs                    # 10,000 NPC stress test
```

## License

This project is for educational and research purposes.

## Acknowledgments

- Original L1J-TW 3.80c Java server as reference
- Game data from Bahamut wiki, LoA 3.63, and official sources