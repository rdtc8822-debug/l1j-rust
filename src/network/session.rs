use anyhow::{bail, Result};
use rand::RngExt;
use sqlx::MySqlPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use crate::config::ServerConfig;
use crate::network::cipher::Cipher;
use crate::network::codec;
use crate::network::shared_state::{SharedWorld, OnlinePlayer};
use crate::protocol::opcodes;

/// 3.80c Taiwan Server first packet payload (after opcode + key).
const FIRST_PACKET: [u8; 11] = [
    0x9d, 0xd1, 0xd6, 0x7a, 0xf4, 0x62, 0xe7, 0xa0, 0x66, 0x02, 0xfa,
];

/// Session state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionState {
    Connected,
    VersionVerified,
    Authenticated,
    InGame,
}

/// Represents a single client connection.
pub struct Session {
    stream: TcpStream,
    cipher: Option<Cipher>,
    pub state: SessionState,
    pub config: ServerConfig,
    pub db: Option<MySqlPool>,
    /// Authenticated account name (set after successful login)
    pub account_name: Option<String>,
    /// Selected character name (set after character selection)
    pub char_name: Option<String>,
    /// Server start time as unix timestamp
    pub server_start_time: i32,
    /// Client IP address
    pub client_ip: String,
    /// Character position (tracked server-side)
    pub char_x: i32,
    pub char_y: i32,
    pub char_map: i32,
    pub char_heading: i32,
    pub char_objid: i32,
    /// Shared world state (for seeing other players)
    pub world: SharedWorld,
    /// Channel to receive packets from other sessions (broadcasts)
    pub packet_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    pub packet_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

impl Session {
    pub fn new(
        stream: TcpStream,
        config: ServerConfig,
        db: Option<MySqlPool>,
        client_ip: String,
        world: SharedWorld,
    ) -> Self {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i32;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Session {
            stream,
            cipher: None,
            state: SessionState::Connected,
            config,
            db,
            account_name: None,
            char_name: None,
            server_start_time: start_time,
            client_ip,
            char_x: 0,
            char_y: 0,
            char_map: 0,
            char_heading: 0,
            char_objid: 0,
            world,
            packet_rx: rx,
            packet_tx: tx,
        }
    }

    /// Send the initial handshake packet (unencrypted).
    async fn send_handshake(&mut self) -> Result<u32> {
        let key: u32 = rand::rng().random_range(1..=0x7FFFFFFFu32);

        let mut payload = Vec::with_capacity(1 + 4 + FIRST_PACKET.len());
        payload.push(opcodes::server::S_OPCODE_INITPACKET);
        payload.push((key & 0xFF) as u8);
        payload.push((key >> 8 & 0xFF) as u8);
        payload.push((key >> 16 & 0xFF) as u8);
        payload.push((key >> 24 & 0xFF) as u8);
        payload.extend_from_slice(&FIRST_PACKET);

        let frame = codec::encode_frame(&payload);
        self.stream.write_all(&frame).await?;
        self.stream.flush().await?;

        debug!("Handshake sent: key=0x{:08X}", key);
        Ok(key)
    }

    /// Read one packet from the client (decrypts if cipher initialized).
    pub async fn read_packet(&mut self) -> Result<Vec<u8>> {
        let lo = self.stream.read_u8().await?;
        let hi = self.stream.read_u8().await?;

        let data_length = match codec::decode_length(lo, hi) {
            Some(len) => len,
            None => bail!("Invalid packet length header: [{}, {}]", lo, hi),
        };

        let mut data = vec![0u8; data_length];
        self.stream.read_exact(&mut data).await?;

        if let Some(ref mut cipher) = self.cipher {
            cipher.decrypt(&mut data);
        }

        Ok(data)
    }

    /// Send one packet to the client (encrypts + pads to 4-byte alignment).
    pub async fn send_packet(&mut self, payload: &[u8]) -> Result<()> {
        let padded_len = (payload.len() + 3) & !3;
        let mut data = vec![0u8; padded_len];
        data[..payload.len()].copy_from_slice(payload);

        if let Some(ref mut cipher) = self.cipher {
            cipher.encrypt(&mut data);
        }

        let frame = codec::encode_frame(&data);
        self.stream.write_all(&frame).await?;
        self.stream.flush().await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

pub async fn handle_session(
    stream: TcpStream,
    config: ServerConfig,
    db: Option<MySqlPool>,
    world: SharedWorld,
) -> Result<()> {
    let client_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();

    let mut session = Session::new(stream, config, db, client_ip, world);

    // Step 1: Send handshake
    let key = session.send_handshake().await?;
    session.cipher = Some(Cipher::new(key));
    info!("Cipher initialized, entering packet loop");

    // Step 2: Main packet loop — handles BOTH client packets and broadcasts from other sessions
    // Split the packet_rx out to avoid borrow conflicts with session in select!
    let mut packet_rx = std::mem::replace(
        &mut session.packet_rx,
        tokio::sync::mpsc::unbounded_channel().1, // dummy rx
    );

    loop {
        tokio::select! {
            // Client sent us a packet
            result = session.read_packet() => {
                let data = match result {
                    Ok(d) => d,
                    Err(e) => {
                        debug!("Connection closed: {}", e);
                        break;
                    }
                };

                if data.is_empty() { continue; }

                let opcode = data[0];
                debug!(
                    "Recv opcode={} (0x{:02X}), len={}, state={:?}",
                    opcode, opcode, data.len(), session.state
                );

                match session.state {
                    SessionState::Connected => handle_connected(&mut session, opcode, &data).await?,
                    SessionState::VersionVerified => handle_version_verified(&mut session, opcode, &data).await?,
                    SessionState::Authenticated => handle_authenticated(&mut session, opcode, &data).await?,
                    SessionState::InGame => handle_in_game(&mut session, opcode, &data).await?,
                }
            }
            // Another session sent us a broadcast packet (e.g., movement, chat)
            Some(broadcast_pkt) = packet_rx.recv() => {
                if let Err(e) = session.send_packet(&broadcast_pkt).await {
                    debug!("Failed to send broadcast: {}", e);
                    break;
                }
            }
        }
    }

    // Cleanup: save character + set account offline
    cleanup_session(&session).await;

    info!("Session ended");
    Ok(())
}

// ---------------------------------------------------------------------------
// State handlers
// ---------------------------------------------------------------------------

async fn handle_connected(session: &mut Session, opcode: u8, data: &[u8]) -> Result<()> {
    if opcode == opcodes::client::C_CLIENTVERSION {
        let cv = crate::protocol::client::login::parse_client_version(data);
        info!(
            "Client version: lang={}, ver=0x{:08X}",
            cv.client_language, cv.client_version
        );

        // Send S_SERVERVERSION
        let pkt = crate::protocol::server::login::build_server_version(session.server_start_time);
        session.send_packet(&pkt).await?;
        session.state = SessionState::VersionVerified;
        info!("State -> VersionVerified");
    } else {
        warn!("Unexpected opcode {} in Connected state", opcode);
    }
    Ok(())
}

async fn handle_version_verified(session: &mut Session, opcode: u8, data: &[u8]) -> Result<()> {
    // Handle both login packet types:
    //   opcode 210 (C_BEANFUNLOGIN) - has action byte prefix
    //   opcode 119 (C_LOGINPACKET)  - direct account+password
    if opcode == opcodes::client::C_BEANFUNLOGIN || opcode == 119 {
        let auth = if opcode == 119 {
            crate::protocol::client::login::parse_login_packet(data)
        } else {
            crate::protocol::client::login::parse_auth_login(data)
        };

        if auth.action != crate::protocol::client::login::LOGIN_ACTION_LOGIN {
            debug!("Auth action {} (not login)", auth.action);
            return Ok(());
        }

        info!("Login attempt: account={}", auth.account);

        // Validate against DB
        let pool = match &session.db {
            Some(p) => p,
            None => {
                warn!("No database - cannot authenticate");
                let pkt = crate::protocol::server::login::build_login_result(
                    crate::protocol::server::login::REASON_ACCESS_FAILED,
                );
                session.send_packet(&pkt).await?;
                return Ok(());
            }
        };

        let account_data = crate::db::account::load_account(pool, &auth.account).await?;

        let account = match account_data {
            Some(a) => a,
            None => {
                // Auto-create account (common in L1J private servers)
                info!("Account not found, auto-creating: {}", auth.account);
                if let Err(e) = crate::db::account::create_account(
                    pool, &auth.account, &auth.password,
                ).await {
                    warn!("Failed to create account: {}", e);
                    let pkt = crate::protocol::server::login::build_login_result(
                        crate::protocol::server::login::REASON_ACCESS_FAILED,
                    );
                    session.send_packet(&pkt).await?;
                    return Ok(());
                }
                info!("Account created: {}", auth.account);
                // Re-load the newly created account
                match crate::db::account::load_account(pool, &auth.account).await? {
                    Some(a) => a,
                    None => return Ok(()),
                }
            }
        };

        // Check banned
        if account.banned != 0 {
            info!("Account banned: {}", auth.account);
            let pkt = crate::protocol::server::login::build_login_result(
                crate::protocol::server::login::REASON_ACCESS_FAILED,
            );
            session.send_packet(&pkt).await?;
            return Ok(());
        }

        // Check already online
        if account.online != 0 {
            info!("Account already in use: {}", auth.account);
            let pkt = crate::protocol::server::login::build_login_result(
                crate::protocol::server::login::REASON_ACCOUNT_IN_USE,
            );
            session.send_packet(&pkt).await?;
            return Ok(());
        }

        // Validate password
        if !crate::db::account::validate_password(&auth.password, &account.password) {
            info!("Wrong password for: {}", auth.account);
            let pkt = crate::protocol::server::login::build_login_result(
                crate::protocol::server::login::REASON_ACCESS_FAILED,
            );
            session.send_packet(&pkt).await?;
            return Ok(());
        }

        // Login success!
        info!("Login OK: {}", auth.account);
        crate::db::account::set_online(pool, &auth.account, &session.client_ip).await?;
        session.account_name = Some(auth.account.clone());

        // Send login result
        let pkt = crate::protocol::server::login::build_login_result(
            crate::protocol::server::login::REASON_LOGIN_OK,
        );
        session.send_packet(&pkt).await?;

        // Send character list
        send_char_list(session).await?;

        session.state = SessionState::Authenticated;
        info!("State -> Authenticated");
    } else {
        debug!("Opcode {} in VersionVerified (not handled)", opcode);
    }
    Ok(())
}

async fn send_char_list(session: &mut Session) -> Result<()> {
    let pool = session.db.as_ref().unwrap();
    let account = session.account_name.as_ref().unwrap();

    let chars = crate::db::character::load_char_list(pool, account).await?;
    let max_slots = crate::DEFAULT_CHARACTER_SLOT;

    // S_CHARAMOUNT
    let pkt = crate::protocol::server::char_list::build_char_amount(chars.len() as i32, max_slots);
    session.send_packet(&pkt).await?;

    // S_CHARSYNACK (SYN)
    let pkt = crate::protocol::server::char_list::build_char_syn();
    session.send_packet(&pkt).await?;

    // S_CHARLIST for each character
    for ch in &chars {
        let pkt = crate::protocol::server::char_list::build_char_pack(ch);
        session.send_packet(&pkt).await?;
    }

    // S_CHARSYNACK (ACK)
    let pkt = crate::protocol::server::char_list::build_char_ack();
    session.send_packet(&pkt).await?;

    info!("Sent {} character(s) to client", chars.len());
    Ok(())
}

async fn handle_authenticated(session: &mut Session, opcode: u8, data: &[u8]) -> Result<()> {
    match opcode {
        opcodes::client::C_LOGINTOSERVER => {
            let req = crate::protocol::client::char_select::parse_login_to_server(data);
            info!("Character selected: {}", req.char_name);

            let pool = match &session.db {
                Some(p) => p,
                None => return Ok(()),
            };
            let account = session.account_name.as_ref().unwrap();

            let ch = crate::db::character::load_character(pool, &req.char_name, account).await?;

            let ch = match ch {
                Some(c) => c,
                None => {
                    warn!("Character not found: {}", req.char_name);
                    return Ok(());
                }
            };

            session.char_name = Some(req.char_name);
            session.char_x = ch.loc_x;
            session.char_y = ch.loc_y;
            session.char_map = ch.map_id;
            session.char_heading = ch.heading;
            session.char_objid = ch.objid;

            // Send ALL game init packets (17+ packets in correct order)
            let init_packets = crate::protocol::server::game_init::build_all_game_init_packets(&ch, 4);
            for pkt in &init_packets {
                session.send_packet(pkt).await?;
            }

            // Register in shared world so other players can see us
            let gfxid = crate::protocol::client::char_create::get_gfx_id(ch.char_type, ch.sex);
            let nearby_packets = {
                let mut world = session.world.lock().await;

                // Collect nearby player packets (can't send while holding lock)
                let nearby = world.get_nearby_players(ch.map_id, ch.loc_x, ch.loc_y, ch.objid);
                let packets: Vec<Vec<u8>> = nearby.iter().map(|p| build_player_charpack(p)).collect();

                // Register ourselves
                let me = OnlinePlayer {
                    object_id: ch.objid,
                    name: ch.char_name.clone(),
                    x: ch.loc_x,
                    y: ch.loc_y,
                    map_id: ch.map_id,
                    heading: ch.heading,
                    gfx_id: gfxid,
                    level: ch.level,
                    lawful: ch.lawful,
                    char_type: ch.char_type,
                    sex: ch.sex,
                    clan_name: ch.clanname.clone(),
                    title: String::new(),
                    packet_tx: session.packet_tx.clone(),
                };

                // Broadcast our appearance to nearby players
                let my_pack = build_player_charpack(&me);
                world.broadcast_to_nearby(ch.map_id, ch.loc_x, ch.loc_y, ch.objid, &my_pack);

                world.add_player(me);
                packets
            };
            // Now send collected packets (lock released)
            for pkt in &nearby_packets {
                session.send_packet(pkt).await?;
            }

            session.state = SessionState::InGame;
            info!(
                "State -> InGame (char={}, map={}, pos={},{}) - sent {} init packets",
                ch.char_name, ch.map_id, ch.loc_x, ch.loc_y, init_packets.len()
            );
        }
        opcodes::client::C_NEWCHAR => {
            info!("Character creation requested");
            handle_create_char(session, data).await?;
        }
        opcodes::client::C_DELETECHAR => {
            info!("Character deletion requested");
            // TODO: Parse char name, mark for deletion in DB
        }
        _ => {
            debug!("Opcode {} in Authenticated (not handled)", opcode);
        }
    }
    Ok(())
}

async fn handle_in_game(session: &mut Session, opcode: u8, data: &[u8]) -> Result<()> {
    match opcode {
        opcodes::client::C_MOVECHAR => {
            let mv = crate::protocol::client::movement::parse_move_char(data);
            let (dx, dy) = crate::ecs::components::position::heading_delta(mv.heading);
            session.char_x += dx;
            session.char_y += dy;
            session.char_heading = mv.heading;

            // Broadcast movement to nearby players
            let move_pkt = crate::protocol::server::movement::build_move_char(
                session.char_objid, session.char_x, session.char_y, mv.heading,
            );
            let mut world = session.world.lock().await;
            world.update_position(session.char_objid, session.char_x, session.char_y, mv.heading);
            world.broadcast_to_nearby(
                session.char_map, session.char_x, session.char_y,
                session.char_objid, &move_pkt,
            );
        }
        opcodes::client::C_CHANGEHEADING => {
            let ch = crate::protocol::client::movement::parse_change_heading(data);
            session.char_heading = ch.heading;
        }
        opcodes::client::C_CHAT => {
            let msg = crate::protocol::client::chat::parse_chat(data);
            let name = session.char_name.as_deref().unwrap_or("Unknown");
            info!("[CHAT] {}: {}", name, msg.text);

            // Build chat packet and send to self + broadcast to nearby
            let pkt = crate::protocol::server::chat::build_normal_chat(
                session.char_objid, msg.chat_type as i32, name, &msg.text,
            );
            session.send_packet(&pkt).await?;

            let world = session.world.lock().await;
            world.broadcast_to_nearby(
                session.char_map, session.char_x, session.char_y,
                session.char_objid, &pkt,
            );
        }
        opcodes::client::C_ATTACK => {
            debug!("Attack received (not fully handled yet)");
        }
        opcodes::client::C_USESKILL => {
            debug!("Skill use received (not fully handled yet)");
        }
        opcodes::client::C_USEITEM => {
            debug!("Item use received (not fully handled yet)");
        }
        opcodes::client::C_KEEPALIVE => {
            // Heartbeat - no response needed
        }
        opcodes::client::C_QUITGAME => {
            info!("Client requested quit");
            // Send disconnect packet before closing
            let pkt = crate::protocol::packet::PacketBuilder::new(
                crate::protocol::opcodes::server::S_OPCODE_DISCONNECT
            ).build();
            let _ = session.send_packet(&pkt).await;
            return Err(anyhow::anyhow!("Client quit"));
        }
        opcodes::client::C_CHANGECHAR => {
            // ESC menu → "重新開始" / return to character select
            info!("Client returning to character select");
            save_character(&session).await;
            session.state = SessionState::Authenticated;
            send_char_list(session).await?;
            info!("State -> Authenticated (restart)");
        }
        opcodes::client::C_RESTARTMENU => {
            // This handles clan ranks, survival cry, etc. (not the ESC menu)
            // For now, just ignore silently
        }
        opcodes::client::C_RESTART => {
            // Restart after death - respawn at saved location
            info!("Client restarting after death");
            // Re-send game init packets at current position
            if let Some(pool) = &session.db {
                if let Some(name) = &session.char_name {
                    let account = session.account_name.as_ref().unwrap();
                    if let Ok(Some(ch)) = crate::db::character::load_character(pool, name, account).await {
                        session.char_x = ch.loc_x;
                        session.char_y = ch.loc_y;
                        session.char_map = ch.map_id;
                        let init_packets = crate::protocol::server::game_init::build_all_game_init_packets(&ch, 4);
                        for pkt in &init_packets {
                            session.send_packet(pkt).await?;
                        }
                    }
                }
            }
        }
        _ => {
            // Silently ignore unhandled opcodes to reduce log spam
        }
    }
    Ok(())
}

/// Save character position to database.
async fn save_character(session: &Session) {
    if let (Some(pool), Some(name)) = (&session.db, &session.char_name) {
        let result = sqlx::query(
            "UPDATE characters SET LocX=?, LocY=?, MapID=?, Heading=? WHERE char_name=?"
        )
        .bind(session.char_x)
        .bind(session.char_y)
        .bind(session.char_map)
        .bind(session.char_heading)
        .bind(name)
        .execute(pool)
        .await;

        match result {
            Ok(_) => info!("Character saved: {} at ({},{} map={})",
                name, session.char_x, session.char_y, session.char_map),
            Err(e) => warn!("Failed to save character: {}", e),
        }
    }
}

/// Cleanup when session ends: remove from world, save character, set account offline.
async fn cleanup_session(session: &Session) {
    // Remove from shared world + broadcast removal to nearby players
    if session.state == SessionState::InGame && session.char_objid != 0 {
        let remove_pkt = crate::protocol::server::npc_pack::build_remove_object(session.char_objid as u32);
        let mut world = session.world.lock().await;
        world.broadcast_to_nearby(
            session.char_map, session.char_x, session.char_y,
            session.char_objid, &remove_pkt,
        );
        world.remove_player(session.char_objid);
        drop(world);

        save_character(session).await;
    }

    // Set account offline
    if let (Some(pool), Some(account)) = (&session.db, &session.account_name) {
        let _ = crate::db::account::set_offline(pool, account).await;
        info!("Account set offline: {}", account);
    }
}

/// Build S_CHARPACK for a player (so other players can see them).
fn build_player_charpack(p: &OnlinePlayer) -> Vec<u8> {
    use crate::protocol::packet::PacketBuilder;
    use crate::protocol::opcodes::server;

    PacketBuilder::new(server::S_OPCODE_CHARPACK)
        .write_h(p.x)
        .write_h(p.y)
        .write_d(p.object_id)
        .write_h(p.gfx_id)
        .write_c(0)              // weapon
        .write_c(p.heading)
        .write_c(0)              // light
        .write_c(0)              // speed
        .write_d(1)              // exp
        .write_h(p.lawful)
        .write_s(Some(&p.name))
        .write_s(Some(&p.title))
        .write_c(4)              // STATUS_PC
        .write_d(0)              // emblem
        .write_s(Some(&p.clan_name))
        .write_s(None)
        .write_c(0xb0_u8 as i32)
        .write_c(0xff_u8 as i32) // party hp
        .write_c(0)
        .write_c(0)
        .write_c(0)
        .write_c(0xff_u8 as i32)
        .write_c(0xff_u8 as i32)
        .write_s(None)
        .write_c(0)
        .build()
}

async fn handle_create_char(session: &mut Session, data: &[u8]) -> Result<()> {
    let nc = crate::protocol::client::char_create::parse_new_char(data);
    info!("Creating character: name={}, type={}, sex={}", nc.name, nc.char_type, nc.sex);

    let pool = match &session.db {
        Some(p) => p,
        None => return Ok(()),
    };
    let account = match &session.account_name {
        Some(a) => a.clone(),
        None => return Ok(()),
    };

    // Validate name
    if nc.name.is_empty() || nc.name.len() > 16 {
        let pkt = crate::protocol::server::char_create::build_char_create_status(
            crate::protocol::server::char_create::REASON_INVALID_NAME,
        );
        session.send_packet(&pkt).await?;
        return Ok(());
    }

    // Check duplicate name
    if crate::db::char_create::name_exists(pool, &nc.name).await? {
        info!("Name already exists: {}", nc.name);
        let pkt = crate::protocol::server::char_create::build_char_create_status(
            crate::protocol::server::char_create::REASON_ALREADY_EXISTS,
        );
        session.send_packet(&pkt).await?;
        return Ok(());
    }

    // Validate stats
    if !crate::protocol::client::char_create::validate_stats(&nc) {
        warn!("Invalid stats for character creation");
        let pkt = crate::protocol::server::char_create::build_char_create_status(
            crate::protocol::server::char_create::REASON_WRONG_AMOUNT,
        );
        session.send_packet(&pkt).await?;
        return Ok(());
    }

    // Generate object ID
    let objid = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() & 0x7FFFFFFF) as i32;

    // Create in database
    match crate::db::char_create::create_character(pool, &account, &nc, objid).await {
        Ok(_) => {
            info!("Character created: {} (objid={})", nc.name, objid);

            let pkt = crate::protocol::server::char_create::build_char_create_status(
                crate::protocol::server::char_create::REASON_OK,
            );
            session.send_packet(&pkt).await?;

            let hp = crate::protocol::client::char_create::get_init_hp(nc.char_type);
            let mp = crate::protocol::client::char_create::calc_init_mp(nc.char_type, nc.wis_stat);
            let pkt = crate::protocol::server::char_create::build_new_char_pack(
                &nc.name, nc.char_type, nc.sex, 0, hp, mp, 10, 1,
                nc.str_stat, nc.dex_stat, nc.con_stat, nc.wis_stat,
                nc.cha_stat, nc.int_stat, 20260207,
            );
            session.send_packet(&pkt).await?;

            send_char_list(session).await?;
        }
        Err(e) => {
            warn!("Failed to create character: {}", e);
            let pkt = crate::protocol::server::char_create::build_char_create_status(
                crate::protocol::server::char_create::REASON_INVALID_NAME,
            );
            session.send_packet(&pkt).await?;
        }
    }
    Ok(())
}
