use std::{
    f32::consts::{PI, TAU},
    net::ToSocketAddrs,
};

use bevy::{app::AppExit, ecs::system::SystemParam, math::vec3, pbr::DirectionalLightShadowMap, prelude::*, utils::HashSet};
use bevy_renet::renet::{transport::NetcodeClientTransport, RenetClient};

use bevy_xpbd_3d::prelude::*;

#[cfg(feature = "target_native_os")]
use bevy_atmosphere::prelude::*;

use crate::ui::prelude::*;
use crate::{client::prelude::*, server::prelude::IntegratedServerPlugin};

use crate::item::{Inventory, ItemPlugin};
use crate::net::{CPacket, ClientNetworkPlugin, RenetClientHelper};
use crate::util::TimeIntervals;
use crate::voxel::ClientVoxelPlugin;

pub struct ClientGamePlugin;

impl Plugin for ClientGamePlugin {
    fn build(&self, app: &mut App) {
        // Render
        {
            // Atmosphere
            #[cfg(feature = "target_native_os")]
            {
                app.add_plugins(AtmospherePlugin);
                app.insert_resource(AtmosphereModel::default());
            }

            // Billiboard
            // use bevy_mod_billboard::prelude::*;
            // app.add_plugins(BillboardPlugin);

            // ShadowMap sizes
            app.insert_resource(DirectionalLightShadowMap { size: 512 });

            // SSAO
            // app.add_plugins(TemporalAntiAliasPlugin);
            // app.insert_resource(AmbientLight { brightness: 0.05, ..default() });
        }
        // .obj model loader.
        app.add_plugins(bevy_obj::ObjPlugin);
        app.insert_resource(GlobalVolume::new(1.0)); // Audio GlobalVolume

        // Physics
        app.add_plugins(PhysicsPlugins::default());

        // UI
        app.add_plugins(crate::ui::UiPlugin);

        // Gameplay
        app.add_plugins(CharacterControllerPlugin); // CharacterController
        app.add_plugins(ClientVoxelPlugin); // Voxel
        app.add_plugins(ItemPlugin); // Items

        // Network
        app.add_plugins(ClientNetworkPlugin); // Network Client
        app.add_plugins(IntegratedServerPlugin);

        // ClientInfo
        app.insert_resource(ClientInfo::default());
        app.insert_resource(ClientSettings::default());
        app.register_type::<ClientInfo>();
        app.register_type::<WorldInfo>();

        // World Setup/Cleanup, Tick
        app.add_systems(First, on_world_init.run_if(condition::load_world)); // Camera, Player, Sun
        app.add_systems(Last, on_world_exit.run_if(condition::unload_world()));
        app.add_systems(Update, tick_world.run_if(condition::in_world)); // Sun, World Timing.

        // Input
        app.add_systems(Startup, super::input::input_setup);
        app.add_systems(Update, super::input::input_handle);
        app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<InputAction>::default());
        // app.add_plugins((bevy_touch_stick::TouchStickPlugin::<InputStickId>::default());

        // App Init/Exit
        app.add_systems(PreStartup, on_app_init); // load settings
        app.add_systems(Last, on_app_exit); // save settings

        // Debug
        {
            app.add_systems(Update, wfc_test);

            // Draw Basis
            app.add_systems(PostUpdate, debug_draw_gizmo.after(PhysicsSet::Sync).run_if(condition::in_world));

            // World Inspector
            app.add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new().run_if(|cli: Res<ClientInfo>| cli.dbg_inspector));
        }
    }
}

fn on_app_init(mut cfg: ResMut<ClientSettings>) {
    info!("Loading {CLIENT_SETTINGS_FILE}");
    if let Ok(str) = std::fs::read_to_string(CLIENT_SETTINGS_FILE) {
        if let Ok(val) = serde_json::from_str(&str) {
            *cfg = val;
        }
    }
}

fn on_app_exit(mut exit_events: EventReader<AppExit>, cfg: Res<ClientSettings>) {
    for _ in exit_events.read() {
        info!("Program Terminate");

        info!("Saving {CLIENT_SETTINGS_FILE}");
        std::fs::write(CLIENT_SETTINGS_FILE, serde_json::to_string_pretty(&*cfg).unwrap()).unwrap();
    }
}

pub mod condition {
    use super::{ClientInfo, WorldInfo};
    use crate::ui::CurrentUI;
    use bevy::ecs::{change_detection::DetectChanges, schedule::common_conditions::resource_removed, system::Res};

    // a.k.a. loaded_world
    pub fn in_world(res: Option<Res<WorldInfo>>, res_vox: Option<Res<crate::voxel::ClientChunkSystem>>) -> bool {
        res.is_some() && res_vox.is_some()
    }
    pub fn load_world(res: Option<Res<WorldInfo>>) -> bool {
        res.is_some_and(|r| r.is_added())
    }
    pub fn unload_world() -> impl FnMut(Option<Res<WorldInfo>>) -> bool + Clone {
        resource_removed::<WorldInfo>()
    }
    pub fn manipulating(cli: Res<ClientInfo>) -> bool {
        cli.curr_ui == CurrentUI::None
    }
    pub fn in_ui(ui: CurrentUI) -> impl FnMut(Res<ClientInfo>) -> bool + Clone {
        move |cli: Res<ClientInfo>| cli.curr_ui == ui
    }
}

/// Marker: Despawn the Entity on World Unload.
#[derive(Component)]
pub struct DespawnOnWorldUnload;

// Marker: Sun
#[derive(Component)]
struct Sun;

#[derive(Component)]
struct WfcTest;

fn wfc_test(
    mut cmds: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,

    mut ctx: bevy_egui::EguiContexts,
    query_wfc: Query<Entity, With<WfcTest>>,

    mut tx_templ_name: Local<String>,
) {
    bevy_egui::egui::Window::new("WFC").show(ctx.ctx_mut(), |ui| {
        ui.text_edit_singleline(&mut *tx_templ_name);

        if ui.btn("ReGen").clicked() {
            for e_wfc in query_wfc.iter() {
                cmds.entity(e_wfc).despawn_recursive();
            }

            use crate::wfc::*;
            let mut wfc = WFC::new();
            wfc.push_pattern("0".into(), [0; 6], false, false);
            wfc.push_pattern("1".into(), [1; 6], false, false);
            wfc.push_pattern("2".into(), [0, 2, 0, 0, 0, 0], true, false);
            wfc.push_pattern("3".into(), [3, 3, 0, 0, 0, 0], true, false);
            wfc.push_pattern("4".into(), [1, 2, 0, 0, 4, 4], true, false);
            wfc.push_pattern("5".into(), [4, 0, 0, 0, 4, 0], true, false);
            wfc.push_pattern("6".into(), [2, 2, 0, 0, 0, 0], true, false);
            wfc.push_pattern("7".into(), [2, 2, 0, 0, 3, 3], true, false);
            wfc.push_pattern("8".into(), [0, 0, 0, 0, 3, 2], true, false);
            wfc.push_pattern("9".into(), [2, 2, 0, 0, 2, 0], true, false);
            wfc.push_pattern("10".into(), [2, 2, 0, 0, 2, 2], true, false);
            wfc.push_pattern("11".into(), [0, 2, 0, 0, 2, 0], true, false);
            wfc.push_pattern("12".into(), [2, 2, 0, 0, 0, 0], true, false);
            wfc.init_tiles(IVec3::new(15, 1, 15));

            wfc.run();

            for tile in wfc.tiles.iter() {
                if tile.entropy() == 0 {
                    continue; // ERROR
                }
                let pat = &wfc.all_patterns[tile.possib[0] as usize];

                cmds.spawn((
                    PbrBundle {
                        mesh: meshes.add(Plane3d::new(Vec3::Y)),
                        material: materials.add(StandardMaterial {
                            base_color_texture: Some(asset_server.load(format!("test/comp/circuit{}/{}.png", &*tx_templ_name, pat.name))),
                            unlit: true,
                            ..default()
                        }),
                        transform: Transform::from_translation(tile.pos.as_vec3() + (Vec3::ONE - Vec3::Y) * 0.5)
                            .with_scale(Vec3::ONE * 0.49 * if pat.is_flipped { -1.0 } else { 1.0 })
                            .with_rotation(Quat::from_axis_angle(Vec3::Y, f32::to_radians(pat.rotation as f32 * 90.0))),
                        ..default()
                    },
                    WfcTest,
                ));
            }
        }
    });
}

fn on_world_init(
    mut cmds: Commands,
    asset_server: Res<AssetServer>,
    materials: ResMut<Assets<StandardMaterial>>,
    meshes: ResMut<Assets<Mesh>>,
    cli: ResMut<ClientInfo>,
) {
    info!("Load World. setup Player, Camera, Sun.");

    // crate::net::netproc_client::spawn_player(
    //     &mut cmds.spawn_empty(),
    //     true,
    //     &cli.cfg.username, &asset_server, &mut meshes, &mut materials);

    // Camera
    cmds.spawn((
        Camera3dBundle {
            projection: Projection::Perspective(PerspectiveProjection { fov: TAU / 4.6, ..default() }),
            camera: Camera { hdr: true, ..default() },
            ..default()
        },
        #[cfg(feature = "target_native_os")]
        AtmosphereCamera::default(), // Marks camera as having a skybox, by default it doesn't specify the render layers the skybox can be seen on
        FogSettings {
            // color, falloff shoud be set in ClientInfo.sky_fog_visibility, etc. due to dynamic debug reason.
            // falloff: FogFalloff::Atmospheric { extinction: Vec3::ZERO, inscattering:  Vec3::ZERO },  // mark as Atmospheric. value will be re-set by ClientInfo.sky_fog...
            ..default()
        },
        CharacterControllerCamera,
        Name::new("Camera"),
        DespawnOnWorldUnload,
    ));
    // .insert(ScreenSpaceAmbientOcclusionBundle::default())
    // .insert(TemporalAntiAliasBundle::default());

    // Sun
    cmds.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight { ..default() },
            ..default()
        },
        Sun, // Marks the light as Sun
        Name::new("Sun"),
        DespawnOnWorldUnload,
    ));
}

fn on_world_exit(mut cmds: Commands, query_despawn: Query<Entity, With<DespawnOnWorldUnload>>) {
    info!("Unload World");

    for entity in query_despawn.iter() {
        cmds.entity(entity).despawn_recursive();
    }

    // todo: net_client.disconnect();  即时断开 否则服务器会觉得你假死 对其他用户体验不太好
    cmds.remove_resource::<RenetClient>();
    cmds.remove_resource::<NetcodeClientTransport>();
}

fn tick_world(
    #[cfg(feature = "target_native_os")] mut atmosphere: AtmosphereMut<Nishita>,

    mut query_sun: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut worldinfo: ResMut<WorldInfo>,
    time: Res<Time>,

    query_player: Query<&Transform, (With<CharacterController>, Without<Sun>)>,
    mut net_client: ResMut<RenetClient>,
    mut last_player_pos: Local<Vec3>,

    mut query_fog: Query<&mut FogSettings>,
    cli: Res<ClientInfo>,
) {
    // worldinfo.tick_timer.tick(time.delta());
    // if !worldinfo.tick_timer.just_finished() {
    //     return;
    // }
    // let dt_sec = worldinfo.tick_timer.duration().as_secs_f32();  // constant time step?

    // // Pause & Steps
    // if worldinfo.is_paused {
    //     if  worldinfo.paused_steps > 0 {
    //         worldinfo.paused_steps -= 1;
    //     } else {
    //         return;
    //     }
    // }
    let dt_sec = time.delta_seconds();

    worldinfo.time_inhabited += dt_sec;

    // DayTime
    if worldinfo.daytime_length != 0. {
        worldinfo.daytime += dt_sec / worldinfo.daytime_length;
        worldinfo.daytime -= worldinfo.daytime.trunc(); // trunc to [0-1]
    }

    // Send PlayerPos
    if let Ok(player_loc) = query_player.get_single() {
        let player_pos = player_loc.translation;

        if player_pos.distance_squared(*last_player_pos) > 0.01 * 0.01 {
            *last_player_pos = player_pos;
            net_client.send_packet(&CPacket::PlayerPos { position: player_pos });
        }
    }
    // net_client.send_packet(&CPacket::LoadDistance {
    //     load_distance: cli.chunks_load_distance,
    // }); // todo: Only Send after Edit Dist Config

    // Ping Network
    if time.at_interval(1.0) {
        net_client.send_packet(&CPacket::Ping {
            client_time: crate::util::current_timestamp_millis(),
            last_rtt: cli.ping.0 as u32,
        });
    }

    // Fog
    let mut fog = query_fog.single_mut();
    fog.color = cli.sky_fog_color;
    if cli.sky_fog_is_atomspheric {
        // let FogFalloff::Atmospheric { .. } = fog.falloff {
        fog.falloff = FogFalloff::from_visibility_colors(cli.sky_fog_visibility, cli.sky_extinction_color, cli.sky_inscattering_color);
    } else {
        fog.falloff = FogFalloff::from_visibility_squared(cli.sky_fog_visibility / 4.0);
    }

    // Sun Pos
    let sun_angle = worldinfo.daytime * PI * 2.;

    #[cfg(feature = "target_native_os")]
    {
        atmosphere.sun_position = Vec3::new(sun_angle.cos(), sun_angle.sin(), 0.);
    }

    if let Some((mut light_trans, mut directional)) = query_sun.single_mut().into() {
        directional.illuminance = sun_angle.sin().max(0.0).powf(2.0) * cli.skylight_illuminance * 1000.0;
        directional.shadows_enabled = cli.skylight_shadow;

        // or from000.looking_at()
        light_trans.rotation = Quat::from_rotation_z(sun_angle) * Quat::from_rotation_y(PI / 2.3);
    }
}

fn debug_draw_gizmo(
    mut gizmo: Gizmos,
    // mut gizmo_config: ResMut<GizmoConfigStore>,
    query_cam: Query<&Transform, With<CharacterControllerCamera>>,
) {
    // gizmo.config.depth_bias = -1.; // always in front

    // World Basis Axes
    let n = 5;
    gizmo.line(Vec3::ZERO, Vec3::X * 2. * n as f32, Color::RED);
    gizmo.line(Vec3::ZERO, Vec3::Y * 2. * n as f32, Color::GREEN);
    gizmo.line(Vec3::ZERO, Vec3::Z * 2. * n as f32, Color::BLUE);

    let color = Color::GRAY;
    for x in -n..=n {
        gizmo.ray(vec3(x as f32, 0., -n as f32), Vec3::Z * n as f32 * 2., color);
    }
    for z in -n..=n {
        gizmo.ray(vec3(-n as f32, 0., z as f32), Vec3::X * n as f32 * 2., color);
    }

    // View Basis
    if let Ok(cam_trans) = query_cam.get_single() {
        // let cam_trans = query_cam.single();
        let p = cam_trans.translation;
        let rot = cam_trans.rotation;
        let n = 0.03;
        let offset = vec3(0., 0., -0.5);
        gizmo.ray(p + rot * offset, Vec3::X * n, Color::RED);
        gizmo.ray(p + rot * offset, Vec3::Y * n, Color::GREEN);
        gizmo.ray(p + rot * offset, Vec3::Z * n, Color::BLUE);
    }
}

/// the resource only exixts when world is loaded

#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct WorldInfo {
    pub seed: u64,

    pub name: String,

    pub daytime: f32,

    // seconds a day time long
    pub daytime_length: f32,

    // seconds
    pub time_inhabited: f32,

    time_created: u64,
    time_modified: u64,

    tick_timer: Timer,

    pub is_paused: bool,
    pub paused_steps: i32,
    // pub is_manipulating: bool,
}

impl Default for WorldInfo {
    fn default() -> Self {
        WorldInfo {
            seed: 0,
            name: "None Name".into(),
            daytime: 0.15,
            daytime_length: 60. * 24.,

            time_inhabited: 0.,
            time_created: 0,
            time_modified: 0,

            tick_timer: Timer::new(bevy::utils::Duration::from_secs_f32(1. / 20.), TimerMode::Repeating),

            is_paused: false,
            paused_steps: 0,
            // is_manipulating: true,
        }
    }
}

// ClientSettings Configs

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct ServerListItem {
    pub name: String,
    pub addr: String,

    #[serde(skip)]
    pub ui: crate::ui::serverlist::UiServerInfo,
}

const CLIENT_SETTINGS_FILE: &str = "client.settings.json";

#[derive(Resource, serde::Deserialize, serde::Serialize, Asset, TypePath)]
pub struct ClientSettings {
    // Name, Addr
    pub serverlist: Vec<ServerListItem>,
    pub fov: f32,
    pub username: String,
    pub hud_padding: f32,
    pub vsync: bool,

    pub chunks_load_distance: IVec2,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            serverlist: Vec::default(),
            fov: 85.,
            username: crate::util::generate_simple_user_name(),
            hud_padding: 24.,
            vsync: true,

            chunks_load_distance: IVec2::new(4, 3),
        }
    }
}

pub const HOTBAR_SLOTS: u32 = 9;

#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct ClientInfo {
    // Networking
    pub server_addr: String, // just a record
    pub disconnected_reason: String,
    pub ping: (u64, i64, i64, u64),     // ping. (rtt, c2s, ping-begin) in ms.
    pub playerlist: Vec<(String, u32)>, // as same as SPacket::PlayerList. username, ping.

    // Debug Draw
    pub dbg_text: bool,
    pub dbg_menubar: bool,
    pub dbg_inspector: bool,
    pub dbg_gizmo_remesh_chunks: bool,
    pub dbg_gizmo_curr_chunk: bool,
    pub dbg_gizmo_all_loaded_chunks: bool,

    // Render Sky
    pub sky_fog_color: Color,
    pub sky_fog_visibility: f32,
    pub sky_inscattering_color: Color,
    pub sky_extinction_color: Color,
    pub sky_fog_is_atomspheric: bool,
    pub skylight_shadow: bool,
    pub skylight_illuminance: f32,

    // Control
    pub enable_cursor_look: bool,

    // ClientPlayerInfo
    #[reflect(ignore)]
    pub inventory: Inventory,

    pub hotbar_index: u32,

    pub health: u32,
    pub health_max: u32,

    // UI
    #[reflect(ignore)]
    pub curr_ui: CurrentUI,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            disconnected_reason: String::new(),
            ping: (0, 0, 0, 0),
            playerlist: Vec::new(),
            server_addr: String::new(),

            dbg_text: false,
            dbg_menubar: true,
            dbg_inspector: false,
            dbg_gizmo_remesh_chunks: true,
            dbg_gizmo_curr_chunk: false,
            dbg_gizmo_all_loaded_chunks: false,

            sky_fog_color: Color::rgba(0.0, 0.666, 1.0, 1.0),
            sky_fog_visibility: 1200.0, // 280 for ExpSq, 1200 for Atmo
            sky_fog_is_atomspheric: true,
            sky_inscattering_color: Color::rgb(110.0 / 255.0, 230.0 / 255.0, 1.0), // bevy demo: Color::rgb(0.7, 0.844, 1.0),
            sky_extinction_color: Color::rgb(0.35, 0.5, 0.66),

            skylight_shadow: false,
            skylight_illuminance: 20.,

            enable_cursor_look: true,

            inventory: Inventory::new(36),
            hotbar_index: 0,
            health: 20,
            health_max: 20,

            curr_ui: CurrentUI::MainMenu,
        }
    }
}

// A helper on Client

#[derive(SystemParam)]
pub struct EthertiaClient<'w, 's> {
    clientinfo: ResMut<'w, ClientInfo>,
    pub cfg: ResMut<'w, ClientSettings>,

    cmds: Commands<'w, 's>,
}

impl<'w, 's> EthertiaClient<'w, 's> {
    /// for Singleplayer
    // pub fn load_world(&mut self, cmds: &mut Commands, server_addr: String)

    pub fn data(&mut self) -> &mut ClientInfo {
        self.clientinfo.as_mut()
    }

    pub fn connect_server(&mut self, server_addr: String) {
        info!("Connecting to {}", server_addr);

        let mut addrs = match server_addr.trim().to_socket_addrs() {
            Ok(addrs) => addrs.collect::<Vec<_>>(),
            Err(err) => {
                error!("Failed to resolve DNS of server_addr: {}", err);
                self.data().curr_ui = CurrentUI::DisconnectedReason;
                return;
            }
        };
        let addr = match addrs.pop() {
            Some(addr) => addr,
            None => {
                self.data().curr_ui = CurrentUI::DisconnectedReason;
                return;
            }
        };

        self.data().curr_ui = CurrentUI::ConnectingServer;
        self.clientinfo.server_addr.clone_from(&server_addr);

        let mut net_client = RenetClient::new(bevy_renet::renet::ConnectionConfig::default());

        let username = &self.cfg.username;
        net_client.send_packet(&CPacket::Login {
            uuid: crate::util::hashcode(username),
            access_token: 123,
            username: username.clone(),
        });

        self.cmds.insert_resource(net_client);
        self.cmds.insert_resource(crate::net::new_netcode_client_transport(
            addr,
            Some("userData123".to_string().into_bytes()),
        ));

        // clear DisconnectReason on new connect, to prevents display old invalid reason.
        self.clientinfo.disconnected_reason.clear();

        // 提前初始化世界 以防用资源时 发现没有被初始化
        self.cmds.insert_resource(WorldInfo::default());
    }

    pub fn enter_world(&mut self) {
        self.cmds.insert_resource(WorldInfo::default());
        self.data().curr_ui = CurrentUI::None;
    }

    pub fn exit_world(&mut self) {
        self.cmds.remove_resource::<WorldInfo>();
        self.data().curr_ui = CurrentUI::MainMenu;
    }
}
