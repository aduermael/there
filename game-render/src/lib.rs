mod terrain;
mod player;
mod rocks;
mod trees;
mod scatter;
mod atmosphere;

pub use terrain::{TerrainRenderer, Uniforms, create_depth_texture, DEPTH_FORMAT};
pub use player::{PlayerRenderer, PlayerInstance, player_color};
pub use rocks::{RockRenderer, RockInstance};
pub use trees::{TreeRenderer, TreeInstance};
pub use scatter::scatter_objects;
pub use atmosphere::{AtmosphereParams, compute_atmosphere};
