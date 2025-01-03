use std::cmp::PartialEq;
use std::sync::{Arc, Mutex};

use hashbrown::HashMap;
#[cfg(feature = "debug")]
use korangar_debug::logging::print_debug;
#[cfg(feature = "debug")]
use korangar_util::texture_atlas::AtlasAllocation;
use ragnarok_packets::{EntityId, TilePosition};
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::loaders::error::LoadError;
use crate::loaders::{ActionLoader, AnimationLoader, MapLoader, ModelLoader, SpriteLoader, TextureLoader};
#[cfg(feature = "debug")]
use crate::threads;
use crate::world::{AnimationData, EntityType, Map};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum LoaderId {
    AnimationData(EntityId),
    Map(String),
}

pub enum LoadableResource {
    AnimationData(Arc<AnimationData>),
    Map { map: Box<Map>, player_position: TilePosition },
}

enum LoadStatus {
    Loading,
    Completed(LoadableResource),
    Failed(LoadError),
}

impl PartialEq for LoadStatus {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

pub struct AsyncLoader {
    action_loader: Arc<ActionLoader>,
    animation_loader: Arc<AnimationLoader>,
    map_loader: Arc<MapLoader>,
    model_loader: Arc<ModelLoader>,
    sprite_loader: Arc<SpriteLoader>,
    texture_loader: Arc<TextureLoader>,
    pending_loads: Arc<Mutex<HashMap<LoaderId, LoadStatus>>>,
    thread_pool: ThreadPool,
}

impl AsyncLoader {
    pub fn new(
        action_loader: Arc<ActionLoader>,
        animation_loader: Arc<AnimationLoader>,
        map_loader: Arc<MapLoader>,
        model_loader: Arc<ModelLoader>,
        sprite_loader: Arc<SpriteLoader>,
        texture_loader: Arc<TextureLoader>,
    ) -> Self {
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(1)
            .thread_name(|_| "async loader".to_string())
            .build()
            .unwrap();

        Self {
            action_loader,
            animation_loader,
            map_loader,
            model_loader,
            sprite_loader,
            texture_loader,
            pending_loads: Arc::new(Mutex::new(HashMap::new())),
            thread_pool,
        }
    }

    pub fn request_animation_data_load(&self, entity_id: EntityId, entity_type: EntityType, entity_part_files: Vec<String>) {
        let sprite_loader = self.sprite_loader.clone();
        let action_loader = self.action_loader.clone();
        let animation_loader = self.animation_loader.clone();

        self.request_load(LoaderId::AnimationData(entity_id), move || {
            let animation_data = match animation_loader.get(&entity_part_files) {
                None => animation_loader.load(&sprite_loader, &action_loader, entity_type, &entity_part_files)?,
                Some(animation_data) => animation_data,
            };

            Ok(LoadableResource::AnimationData(animation_data))
        });
    }

    pub fn request_map_load(
        &self,
        map_name: String,
        player_position: TilePosition,
        #[cfg(feature = "debug")] tile_texture_mapping: Arc<Vec<AtlasAllocation>>,
    ) {
        let map_loader = self.map_loader.clone();
        let model_loader = self.model_loader.clone();
        let texture_loader = self.texture_loader.clone();

        self.request_load(LoaderId::Map(map_name.clone()), move || {
            let map = map_loader.load(
                map_name,
                &model_loader,
                texture_loader,
                #[cfg(feature = "debug")]
                &tile_texture_mapping,
            )?;
            Ok(LoadableResource::Map { map, player_position })
        });
    }

    fn request_load<F>(&self, id: LoaderId, load_function: F)
    where
        F: FnOnce() -> Result<LoadableResource, LoadError> + Send + 'static,
    {
        let pending_loads = Arc::clone(&self.pending_loads);

        pending_loads.lock().unwrap().insert(id.clone(), LoadStatus::Loading);

        self.thread_pool.spawn(move || {
            #[cfg(feature = "debug")]
            let _measurement = threads::Loader::start_frame();

            let result = load_function();

            let mut pending_loads = pending_loads.lock().unwrap();

            if !pending_loads.contains_key(&id) {
                return;
            }

            let status = match result {
                Ok(resource) => LoadStatus::Completed(resource),
                Err(err) => LoadStatus::Failed(err),
            };

            pending_loads.insert(id, status);
        });
    }

    pub fn take_completed(&self) -> impl Iterator<Item = (LoaderId, LoadableResource)> + '_ {
        std::iter::from_fn({
            let pending_loads = Arc::clone(&self.pending_loads);

            move || {
                let mut pending_loads = pending_loads.lock().unwrap();

                let completed_id = pending_loads
                    .iter()
                    .find(|(_, status)| matches!(status, LoadStatus::Completed(_) | LoadStatus::Failed(_)))
                    .map(|(id, _)| id.clone());

                if let Some(id) = completed_id {
                    match pending_loads.remove(&id).unwrap() {
                        LoadStatus::Failed(_error) => {
                            #[cfg(feature = "debug")]
                            print_debug!("Async load error: {:?}", _error);
                            None
                        }
                        LoadStatus::Completed(resource) => Some((id, resource)),
                        _ => unreachable!(),
                    }
                } else {
                    None
                }
            }
        })
    }
}
