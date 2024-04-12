use crate::{
    game_client::{ClientInfo, EthertiaClient, ServerListItem},
    game_server::{self, rcon::Motd},
    util,
};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use bevy_egui::{
    egui::{self, Color32, Layout},
    EguiContexts,
};
use bevy_renet::renet::RenetClient;

use super::{sfx_play, ui_lr_panel, CurrentUI, UiExtra};

use super::new_egui_window;



// 
pub fn ui_connecting_server(mut ctx: EguiContexts, mut cli: ResMut<ClientInfo>, mut net_client: ResMut<RenetClient>) {
    new_egui_window("Server List").show(ctx.ctx_mut(), |ui| {
        let h = ui.available_height();

        ui.vertical_centered(|ui| {
            ui.add_space(h * 0.2);

            if net_client.is_connected() {
                ui.label("Authenticating & logging in...");
            } else {
                ui.label("Connecting server...");
            }
            ui.add_space(38.);
            ui.spinner();

            ui.add_space(h * 0.3);

            if ui.btn_normal("Cancel").clicked() {
                // todo: Interrupt Connection without handle Result.
                cli.curr_ui = CurrentUI::MainMenu;
                net_client.disconnect();
            }
        });
    });
}

pub fn ui_disconnected_reason(
    mut ctx: EguiContexts,
    mut cli: ResMut<ClientInfo>, // readonly. mut only for curr_ui.
) {
    new_egui_window("Disconnected Reason").show(ctx.ctx_mut(), |ui| {
        let h = ui.available_height();

        ui.vertical_centered(|ui| {
            ui.add_space(h * 0.2);

            ui.label("Disconnected:");
            ui.colored_label(Color32::WHITE, cli.disconnected_reason.as_str());

            ui.add_space(h * 0.3);

            if ui.btn_normal("Back to title").clicked() {
                cli.curr_ui = CurrentUI::MainMenu;
            }
        });
    });
}




#[derive(Default, Debug)]
pub struct UiServerInfo {
    pub motd: String,
    pub num_players_online: u32,
    pub num_players_limit: u32,
    pub ping: u32,

    pub is_editing: bool,
    pub refreshing_task: Option<(Task<anyhow::Result<Motd>>, u64)>
}

pub fn ui_serverlist(
    mut ctx: EguiContexts,
    mut cli: EthertiaClient,
    // mut refreshing_indices: Local<HashMap<usize, (Task<anyhow::Result<Motd>>, u64)>>,
) {
    new_egui_window("Server List").show(ctx.ctx_mut(), |ui| {
        let serverlist = &mut cli.data().cfg.serverlist;

        // all access defer to one closure.
        let do_new_server = std::cell::Cell::new(false);
        let do_refresh_all = std::cell::Cell::new(false);
        let mut do_stop_refreshing = false;
        let mut do_acquire_list = false;
        let mut do_join_addr = None;
        let mut do_del_idx = None;

        // let show_stop_refresh = !refreshing_indices.is_empty();
        ui_lr_panel(
            ui,
            true,
            |ui| {
                if ui.btn_borderless("Add Server").clicked() {
                    do_new_server.set(true);
                }
                if ui.btn_borderless("Refresh All").clicked() {
                    do_refresh_all.set(true);
                }
                // if show_stop_refresh && ui.btn_borderless("Stop Refresh").clicked() {
                //     do_stop_refreshing = true;
                // }
                ui.separator();
                if ui.btn_borderless("Aquire List").clicked() {
                    do_acquire_list = true;
                }
                if ui.btn_borderless("Direct Connect").clicked() {}
            },
            |ui| {
                for (idx, server_item) in serverlist.iter_mut().enumerate() {
                    let ui_server_info = &mut server_item.ui;

                    let is_editing = ui_server_info.is_editing;
                    let is_accessable = ui_server_info.ping != 0;
                    let mut is_refreshing = ui_server_info.refreshing_task.is_some();

                    ui.group(|ui| {
                        // First Line
                        ui.horizontal(|ui| {
                            if is_editing {
                                ui.text_edit_singleline(&mut server_item.name);
                            } else {
                                // Left: Name
                                ui.colored_label(Color32::WHITE, server_item.name.clone())
                                    .on_hover_text(server_item.addr.clone());
                                ui.small(&server_item.addr);

                                // Right: Status
                                if is_accessable {
                                    ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                        ui.label(format!(
                                            "{}ms · {}/{}",
                                            ui_server_info.ping, ui_server_info.num_players_online, ui_server_info.num_players_limit
                                        ));
                                    });
                                }
                            }
                        });
                        // Second Line
                        ui.horizontal(|ui| {
                            // Description/Motd
                            if is_editing {
                                ui.text_edit_singleline(&mut server_item.addr);
                            } else if is_refreshing {
                                ui.spinner();
                            } else if is_accessable {
                                ui.label(&ui_server_info.motd);
                            } else {
                                ui.colored_label(Color32::DARK_RED, "Inaccessible 🚫");
                            }

                            // Operations
                            ui.with_layout(Layout::right_to_left(egui::Align::Max), |ui| {
                                if is_editing {
                                    if sfx_play(ui.button("✅")).clicked() {
                                        ui_server_info.is_editing = false;
                                    }
                                } else {
                                    if sfx_play(ui.button("🗑")).on_hover_text("Delete").clicked() {
                                        do_del_idx = Some(idx);
                                    }
                                    if sfx_play(ui.button("⛭")).on_hover_text("Edit").clicked() {
                                        ui_server_info.is_editing = true;
                                    }
                                    if is_refreshing {
                                        if sfx_play(ui.button("❌")).on_hover_text("Stop Refreshing").clicked() {
                                            // refreshing_indices.remove(&idx);
                                            is_refreshing = false;
                                            ui_server_info.refreshing_task = None;
                                        }
                                    } else if sfx_play(ui.button("⟲")).on_hover_text("Refresh Server Status").clicked() {
                                        is_refreshing = true;
                                    }
                                    if sfx_play(ui.button("▶")).on_hover_text("Join & Play").clicked() {
                                        do_join_addr = Some(server_item.addr.clone());
                                    }
                                }
                            });
                        });
                    });

                    if is_refreshing || do_refresh_all.get() {
                        let addr = server_item.addr.clone(); // opt
                        let (task, time) = ui_server_info.refreshing_task.get_or_insert_with(|| {
                            (
                                AsyncComputeTaskPool::get()
                                    .spawn(async move { util::http_get_json::<game_server::rcon::Motd>(&format!("http://{}", addr)) }),
                                util::current_timestamp_millis(),
                            )
                        });

                        if task.is_finished() {
                            // let (task, time) = refreshing_indices.remove(&idx).unwrap();

                            match futures_lite::future::block_on(futures_lite::future::poll_once(task)).unwrap() {
                                Ok(r) => {
                                    ui_server_info.motd = r.motd;
                                    ui_server_info.num_players_limit = r.num_player_limit;
                                    ui_server_info.num_players_online = r.num_player_online;
                                    ui_server_info.ping = (util::current_timestamp_millis() - *time) as u32;
                                }
                                Err(err) => {
                                    info!("Failed to access server status: {}", err);
                                }
                            }

                            ui_server_info.refreshing_task = None;
                        }
                    }
                }

                if do_new_server.get() {
                    serverlist.push(ServerListItem {
                        name: "Server Name".into(),
                        addr: "0.0.0.0:4000".into(),
                        ..default()
                    });
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            },
        );

        // if do_stop_refreshing {
        //     refreshing_indices.clear();
        // }

        if do_acquire_list {
            match crate::util::http_get_json("https://ethertia.com/server-info.json") {
                Ok(ret) => *serverlist = ret,
                Err(err) => info!("{}", err),
            }
        }

        if let Some(idx) = do_del_idx {
            serverlist.remove(idx);
        }

        if let Some(addr) = do_join_addr {
            // 连接服务器 这两个操作会不会有点松散
            cli.data().curr_ui = CurrentUI::ConnectingServer;
            cli.connect_server(addr);
        }
    });
}

pub fn ui_localsaves(mut ctx: EguiContexts, mut cli: ResMut<ClientInfo>) {
    new_egui_window("Local Worlds").show(ctx.ctx_mut(), |ui| {
        ui_lr_panel(
            ui,
            false,
            |ui| {
                if sfx_play(ui.selectable_label(false, "New World")).clicked() {
                    cli.curr_ui = CurrentUI::LocalWorldNew;
                }
                if sfx_play(ui.selectable_label(false, "Refresh")).clicked() {}
            },
            |ui| {
                for i in 0..28 {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(Color32::WHITE, "World Name").on_hover_text(
                                "Path: /Saves/Saa
Size: 10.3 MiB
Time Modified: 2024.02.01 11:20 AM
Time Created: 2024.02.01 11:20 AM
Inhabited: 10.3 hours",
                            );
                            ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label("3 days ago · 13 MB");
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Survival · Cheats");
                            ui.with_layout(Layout::right_to_left(egui::Align::Max), |ui| {
                                sfx_play(ui.button("🗑")).on_hover_text("Delete").clicked();
                                sfx_play(ui.button("⛭")).on_hover_text("Edit").clicked();
                                if sfx_play(ui.button("▶")).on_hover_text("Play").clicked() {}
                            });
                        });
                    });
                }
            },
        );
    });
}

#[derive(Default, Debug, PartialEq)]
pub enum Difficulty {
    Peace,
    #[default]
    Normal,
    Hard,
}

pub fn ui_create_world(
    mut ctx: EguiContexts,
    mut cli: ResMut<ClientInfo>,
    mut tx_world_name: Local<String>,
    mut tx_world_seed: Local<String>,
    mut _difficulty: Local<Difficulty>,
) {
    new_egui_window("New World").show(ctx.ctx_mut(), |ui| {
        // ui_lr_panel(ui, true, |ui| {
        //     if sfx_play(ui.selectable_label(true, "General")).clicked() {

        //     }
        //     if sfx_play(ui.selectable_label(false, "Generation")).clicked() {

        //     }
        //     if sfx_play(ui.selectable_label(false, "Gamerules")).clicked() {

        //     }
        // }, |ui| {
        let space = 14.;

        ui.label("Name:");
        sfx_play(ui.text_edit_singleline(&mut *tx_world_name));
        ui.add_space(space);

        ui.label("Seed:");
        sfx_play(ui.text_edit_singleline(&mut *tx_world_seed));
        ui.add_space(space);

        ui.label("Gamemode:");
        ui.horizontal(|ui| {
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Peace, "Survival"));
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Normal, "Creative"));
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Hard, "Spectator"));
        });
        ui.add_space(space);

        ui.label("Difficulty:");
        ui.horizontal(|ui| {
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Peace, "Peace"));
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Normal, "Normal"));
            sfx_play(ui.radio_value(&mut *_difficulty, Difficulty::Hard, "Hard"));
        });
        ui.add_space(space);

        ui.label("Difficulty:");
        egui::ComboBox::from_id_source("Difficulty")
            .selected_text(format!("{:?}", *_difficulty))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut *_difficulty, Difficulty::Peace, "Peace");
                ui.selectable_value(&mut *_difficulty, Difficulty::Normal, "Normal");
                ui.selectable_value(&mut *_difficulty, Difficulty::Hard, "Hard");
            });

        ui.add_space(space);

        ui.add_space(22.);

        sfx_play(ui.add_sized([290., 26.], egui::Button::new("Create World").fill(Color32::DARK_GREEN))).clicked();
        ui.add_space(4.);
        if sfx_play(ui.add_sized([290., 20.], egui::Button::new("Cancel"))).clicked() {
            cli.curr_ui = CurrentUI::LocalWorldList;
        }
        // });
    });
}
