pub mod character_controller;
pub mod game_client;
pub mod ui;

mod input;

pub mod prelude {
    use super::*;
    pub use game_client::{condition, ClientGamePlugin, ClientInfo, ClientSettings, DespawnOnWorldUnload, EthertiaClient, ServerListItem, WorldInfo};
    pub use input::InputAction;

    pub use character_controller::{CharacterController, CharacterControllerBundle, CharacterControllerCamera, CharacterControllerPlugin};
}

#[cfg(feature = "target_native_os")]
pub mod editor;
