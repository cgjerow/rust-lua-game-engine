use crate::bitmaps::vecbool_to_u8;
use crate::camera_2d::Camera2D;
use crate::camera_3d::{Camera3D, CameraAction};
use crate::components_systems::physics_2d::{
    self, Area2D, BodyType, ColliderComponent, FlipComponent, PhysicsBody, Shape,
    TransformComponent,
};
use crate::components_systems::{
    animation_system_update_frames, damage, set_entity_state, ActionState, ActionStateComponent,
    Animation, AnimationComponent, Entity, HealthComponent, SpriteSheetComponent,
};
use crate::graphics::Graphics;
use crate::lua_scriptor::LuaExtendedExecutor;
use crate::texture::Texture;
use crate::world::{AreaInfo, AreaRole, World};
use crate::{debug, graphics_2d, graphics_3d};
use cgmath::Vector2;
use debug::Debug;
use graphics_2d::Graphics2D;
use graphics_3d::Graphics3D;
use mlua::{Result, Table};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

static SAFETY_MAX_FOR_DEV: u64 = 10000;

pub struct Engine {
    player: Entity,
    mouse_pos: [f32; 2], // TODO!!!
    //
    window: Option<Arc<Window>>,
    graphics: Option<Box<dyn Graphics>>,
    camera_mode: CameraOption,
    dimensions: Dimensions,
    count: u64,
    #[allow(dead_code)]
    fps_specified: bool,
    physics_iterations_per_frame: u8,
    target_rate: Option<Duration>,
    last_frame: Instant,
    debugger: Debug,
    asset_cache: HashMap<String, Texture>,
    lua_context: LuaExtendedExecutor,
    world: World,
    width: u32,
    height: u32,
}

pub struct EngineConfig {
    pub fps: String,
    pub debug_enabled: bool,
    pub width: u32,
    pub height: u32,
    pub dimensions: Dimensions,
    pub camera: CameraOption,
}

#[derive(Debug, PartialEq)]
pub enum Dimensions {
    Two,
    Three,
}

#[derive(Debug, PartialEq)]
pub enum CameraOption {
    Follow,
    Independent,
}

impl Engine {
    pub fn new(config: EngineConfig, lua_executor: LuaExtendedExecutor) -> Self {
        let fps_opt = if config.fps.trim().eq_ignore_ascii_case("auto") {
            None
        } else {
            config.fps.parse::<u64>().ok()
        };

        let target_rate = fps_opt.map(|fps| Duration::from_millis(1000 / fps));

        Self {
            mouse_pos: [0.0, 0.0],
            player: Entity(0),
            lua_context: lua_executor,
            window: None,
            graphics: None,
            camera_mode: config.camera,
            dimensions: config.dimensions,
            debugger: Debug::new(config.debug_enabled),
            count: 0,
            fps_specified: fps_opt != None,
            target_rate: target_rate,
            physics_iterations_per_frame: 1,
            last_frame: Instant::now() - target_rate.unwrap_or_default(),
            asset_cache: HashMap::new(),
            width: config.width,
            height: config.height,
            world: World::new(),
        }
    }

    fn get_texture(&mut self, path: &str) -> Texture {
        let texture = self.asset_cache.entry(path.to_string()).or_insert_with(|| {
            debug_log!(self.debugger, "Initialized asset: {}", path);
            self.graphics
                .as_mut()
                .expect("Graphics not initialized")
                .load_texture_from_path(&format!("./src/assets/{}", path))
        });

        texture.clone()
    }

    fn flip(&mut self, entity: u32, x: bool, y: bool) {
        self.world
            .flips
            .insert(Entity(entity), FlipComponent { x, y });
    }

    pub fn update_camera_follow(&mut self, entity: &Entity) {
        if self.dimensions == Dimensions::Two {
            if let Some(transform) = self.world.physics_bodies_2d.get(entity) {
                let graphics = match &mut self.graphics {
                    Some(canvas) => canvas,
                    None => return,
                };
                graphics.move_camera_for_follow(
                    [transform.position[0], transform.position[1], 0.0],
                    [transform.velocity[0], transform.velocity[1], 0.0],
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0],
                );
            }
        }
    }

    fn is_targetting_fps(&self) -> bool {
        return self.fps_specified;
    }

    fn redraw(&self) {
        if self.is_targetting_fps() {
            self.window
                .as_ref()
                .expect("Window does not exist")
                .request_redraw();
        }
    }

    fn update(&mut self, dt: Duration) -> anyhow::Result<()> {
        let update: mlua::Function = self.lua_context.get_function("ENGINE_update");

        let dt32 = dt.as_secs_f32();

        let _ = update.call::<()>(dt32);

        if self.dimensions == Dimensions::Two {
            let incremented_dt = dt32 / self.physics_iterations_per_frame as f32;
            for _i in 0..self.physics_iterations_per_frame {
                let next_transforms = physics_2d::transform_system_calculate_intended_position(
                    &self.world,
                    incremented_dt,
                );
                let collisions = physics_2d::collision_system(&self.world, &next_transforms);
                physics_2d::resolve_collisions(&mut self.world, collisions.clone());
                let collisions_table = self
                    .lua_context
                    .rust_collisions_to_lua_2d(collisions)
                    .unwrap();
                self.lua_context
                    .get_function("ENGINE_on_collision")
                    .call::<()>(collisions_table)
                    .expect("Error handling collisions");

                physics_2d::transform_system_physics(&mut self.world, incremented_dt);
                let after_physics: mlua::Function =
                    self.lua_context.get_function("ENGINE_after_physics");
                let _ = after_physics.call::<()>(incremented_dt);
            }
            self.world.clear_forces();

            if self.camera_mode == CameraOption::Follow {
                self.update_camera_follow(&self.player.clone());
            }
        }

        animation_system_update_frames(&mut self.world, dt32);

        let graphics = match &mut self.graphics {
            Some(canvas) => canvas,
            None => return Ok(()),
        };

        graphics.update_camera();

        return Ok(());
    }

    fn cleanup(&mut self) {
        debug_log!(self.debugger, "Cleaned it? {}", true)
    }

    fn get_window_size(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    fn apply_force_2d(&mut self, id: u32, fx: f32, fy: f32) {
        if self.dimensions == Dimensions::Two {
            self.world
                .physics_bodies_2d
                .get_mut(&Entity(id))
                .unwrap()
                .apply_force(cgmath::Vector2 { x: fx, y: fy });
        }
    }

    fn apply_impulse_2d(&mut self, id: u32, fx: f32, fy: f32) {
        self.world
            .physics_bodies_2d
            .get_mut(&Entity(id))
            .unwrap()
            .apply_impulse(cgmath::Vector2 { x: fx, y: fy });
    }

    fn set_velocity_2d(&mut self, id: u32, vx: f32, vy: f32) {
        self.world
            .physics_bodies_2d
            .get_mut(&Entity(id))
            .unwrap()
            .velocity = Vector2::new(vx, vy);
    }

    fn apply_masks_and_layers(&mut self, id: u32, masks: Table, layers: Table) {
        let masks = vecbool_to_u8(LuaExtendedExecutor::table_to_vec_8(masks));
        let layers = vecbool_to_u8(LuaExtendedExecutor::table_to_vec_8(layers));

        self.world
            .update_area_masks_and_layers(&Entity(id), masks, layers);
    }

    fn get_velocity_2d(&mut self, id: u32) -> [f32; 2] {
        if self.dimensions == Dimensions::Two {
            let velocity = self
                .world
                .physics_bodies_2d
                .get(&Entity(id))
                .unwrap()
                .velocity;
            return [velocity.x, velocity.y];
        }
        [0.0, 0.0]
    }

    fn get_position_2d(&mut self, id: u32) -> [f32; 2] {
        if self.dimensions == Dimensions::Two {
            let position = self
                .world
                .physics_bodies_2d
                .get(&Entity(id))
                .unwrap()
                .position;
            return [position.x, position.y];
        }
        [0.0, 0.0]
    }

    fn apply_move_2d(&mut self, id: u32, x: f32, y: f32) {
        let b = self.world.physics_bodies_2d.get_mut(&Entity(id)).unwrap();
        b.position += Vector2::new(x, y);
    }

    fn damage(&mut self, id: u32, amount: u16) -> bool {
        damage(&mut self.world, &Entity(id), amount)
    }

    fn get_health_table(&self, id: u32) -> Table {
        let h = self
            .world
            .health_bars
            .get(&Entity(id))
            .unwrap_or(&HealthComponent {
                total: 0,
                current: 0,
            })
            .clone();
        let health = self.lua_context.create_table();
        let _ = health.set("total", h.total);
        let _ = health.set("current", h.current);
        health
    }

    fn set_state(&mut self, id: u32, state: u8) {
        set_entity_state(
            &mut self.world,
            Entity(id),
            ActionState::from(state.clone()),
        );
    }

    fn create_body(&mut self, lua_element: mlua::Table) -> [u32; 2] {
        let state: ActionState = lua_element.get("state").unwrap_or(0).into();
        let is_pc: bool = lua_element.get("is_pc").unwrap_or(false).into();
        let x: f32 = lua_element.get("x").unwrap_or(0.0);
        let y: f32 = lua_element.get("y").unwrap_or(0.0);
        let _z: f32 = lua_element.get("z").unwrap_or(0.0);
        let width: f32 = lua_element.get("width").unwrap_or(1.0);
        let height: f32 = lua_element.get("height").unwrap_or(1.0);
        let _depth: f32 = lua_element.get("depth").unwrap_or(1.0);
        let health: u16 = lua_element.get("total_health").unwrap_or(10);
        let collision_box: mlua::Table = lua_element
            .get("collision_box")
            .unwrap_or(self.lua_context.create_table());

        let masks = LuaExtendedExecutor::table_to_vec_8(
            lua_element
                .get("masks")
                .unwrap_or(self.lua_context.create_table()),
        );

        let layers = LuaExtendedExecutor::table_to_vec_8(
            lua_element
                .get("layers")
                .unwrap_or(self.lua_context.create_table()),
        );

        let animations: mlua::Table = lua_element
            .get("animations")
            .unwrap_or(self.lua_context.create_table());

        let entity: Entity = self.world.new_entity();
        if is_pc {
            self.player = entity.clone();
        }
        let mut animations_map = HashMap::new();

        for pair in animations.clone().pairs::<mlua::Value, mlua::Table>() {
            if let Ok((key, tbl)) = pair {
                let numeric_key =
                    key.as_u32()
                        .expect("Numeric key required for Action States") as u8;
                let (mut animation, sprite_path) = Animation::from_lua_table(tbl);
                let action_state = ActionState::from(numeric_key);

                let sprite_id: Entity = self.world.new_entity();
                let texture = self.get_texture(&sprite_path);
                animation.sprite_sheet_id = sprite_id;

                self.world.sprite_sheets.insert(
                    sprite_id.clone(),
                    SpriteSheetComponent {
                        texture_id: sprite_path,
                        texture,
                    },
                );
                animations_map.insert(action_state, animation);
            }
        }

        let current_frame = animations_map.get(&state).unwrap().frames[0].clone();
        let mut collider: Entity = Entity(0);

        if self.dimensions == Dimensions::Two {
            self.world.animations.insert(
                entity.clone(),
                AnimationComponent {
                    animations: animations_map,
                    current_frame_index: 0,
                    current_frame,
                    frame_timer: 0.0,
                },
            );
            self.world.physics_bodies_2d.insert(
                entity.clone(),
                PhysicsBody {
                    shape: Shape::Rectangle {
                        half_extents: cgmath::Vector2 {
                            x: width / 2.0,
                            y: height / 2.0,
                        },
                    },
                    mass: 1.0,
                    body_type: BodyType::from(lua_element.get("type").unwrap_or(1)),
                    velocity: cgmath::Vector2 { x: 0.0, y: 0.0 },
                    position: cgmath::Vector2 { x, y },
                    force_accumulator: cgmath::Vector2 { x: 0.0, y: 0.0 },
                },
            );
            self.world.transforms_2d.insert(
                entity.clone(),
                TransformComponent {
                    position: [x, y],
                    velocity: [0.0, 0.0],
                    acceleration: [0.0, 0.0],
                    size: [width, height],
                },
            );

            if collision_box.get("enabled").unwrap_or(false) {
                collider = self.world.insert_area_2d(
                    AreaInfo {
                        role: AreaRole::Physics,
                        parent: entity.clone(),
                    },
                    Area2D {
                        shape: Shape::Rectangle {
                            half_extents: cgmath::Vector2 {
                                x: width / 2.0,
                                y: height / 2.0,
                            },
                        },
                        size: Vector2 {
                            x: width * collision_box.get("size_modifier_x").unwrap_or(0.0),
                            y: height * collision_box.get("size_modifier_y").unwrap_or(0.0),
                        },
                        offset: Vector2 {
                            x: collision_box.get("offset_x").unwrap_or(0.0),
                            y: (collision_box.get("offset_y").unwrap_or(0.0)),
                        },
                        masks: vecbool_to_u8(masks),
                        layers: vecbool_to_u8(layers),
                    },
                );
            }
            self.world.health_bars.insert(
                entity.clone(),
                HealthComponent {
                    total: health.clone(),
                    current: health.clone(),
                },
            );
            self.world
                .action_states
                .insert(entity.clone(), ActionStateComponent { state });
        }
        [entity.into(), collider.into()]
    }

    pub fn screen_to_world(&self, loc: [f32; 2]) -> [f32; 2] {
        let size = self.get_window_size();
        let half_screen = [size[0] as f32 * 0.5, size[1] as f32 * 0.5];
        // Pixel offset from screen center
        let offset = [loc[0] - half_screen[0], loc[1] - half_screen[1]];

        let graphics = match &self.graphics {
            Some(canvas) => canvas,
            None => return [0.0, 0.0],
        };

        let camera = graphics.get_camera_info();
        // World units per pixel
        let units_per_pixel = (camera.zoom * 2.0) / size[1] as f32;

        // Scaled world offset
        let world_offset = [offset[0] * units_per_pixel, offset[1] * units_per_pixel];

        // Final world position is camera center + offset
        [
            camera.position[0] + world_offset[0],
            camera.position[1] - world_offset[1], // Y is flipped (screen Y-down vs world Y-up)
        ]
    }

    pub fn configure_camera(&mut self, _config: mlua::Table) -> Result<()> {
        // let config = LuaCameraConfig::from_lua_table(config)?;
        // Create camera
        // let camera = Camera3D::new(self.width, self.height, mode.clone());
        //
        /*
        let graphics = match &mut self.graphics {
            Some(canvas) => canvas,
            None => return Ok(()),
        };
        */
        Ok(())
    }

    fn setup(&mut self) {
        let self_ptr = self as *mut Self;
        let get_window_size = self
            .lua_context
            .lua
            .create_function(move |_, ()| {
                let engine = unsafe { &*self_ptr };
                Ok(engine.get_window_size())
            })
            .expect("Could not create function");
        let create_body = self
            .lua_context
            .lua
            .create_function(move |_, element: mlua::Table| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.create_body(element))
            })
            .expect("Could not create function");
        let configure_camera = self
            .lua_context
            .lua
            .create_function(move |_, config: mlua::Table| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.configure_camera(config))
            })
            .expect("Could not create function");
        let apply_force_2d = self
            .lua_context
            .lua
            .create_function(move |_, (id, x, y): (u32, f32, f32)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.apply_force_2d(id, x, y))
            })
            .expect("Could not create function");
        let apply_impulse_2d = self
            .lua_context
            .lua
            .create_function(move |_, (id, x, y): (u32, f32, f32)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.apply_impulse_2d(id, x, y))
            })
            .expect("Could not create function");
        let apply_move_2d = self
            .lua_context
            .lua
            .create_function(move |_, (id, x, y): (u32, f32, f32)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.apply_move_2d(id, x, y))
            })
            .expect("Could not create function");
        let apply_masks_and_layers = self
            .lua_context
            .lua
            .create_function(move |_, (id, masks, layers): (u32, Table, Table)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.apply_masks_and_layers(id, masks, layers))
            })
            .expect("Could not create function");
        let set_velocity_2d = self
            .lua_context
            .lua
            .create_function(move |_, (id, x, y): (u32, f32, f32)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.set_velocity_2d(id, x, y))
            })
            .expect("Could not create function");
        let get_velocity_2d = self
            .lua_context
            .lua
            .create_function(move |_, id: u32| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.get_velocity_2d(id))
            })
            .expect("Could not create function");
        let get_position_2d = self
            .lua_context
            .lua
            .create_function(move |_, id: u32| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.get_position_2d(id))
            })
            .expect("Could not create function");

        let damage = self
            .lua_context
            .lua
            .create_function(move |_, (id, amount): (u32, u16)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.damage(id, amount))
            })
            .expect("Could not create function");
        let get_health = self
            .lua_context
            .lua
            .create_function(move |_, id: u32| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.get_health_table(id))
            })
            .expect("Could not create function");

        let set_state = self
            .lua_context
            .lua
            .create_function(move |_, (id, state): (u32, u8)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.set_state(id, state))
            })
            .expect("Could not create function");
        let flip = self
            .lua_context
            .lua
            .create_function(move |_, (id, x, y): (u32, bool, bool)| {
                let engine = unsafe { &mut *self_ptr };
                Ok(engine.flip(id, x, y))
            })
            .expect("Could not create function");

        let lua_engine = self.lua_context.create_table();
        lua_engine
            .set("get_window_size", get_window_size)
            .expect("Could not set engine function");
        lua_engine
            .set("flip", flip)
            .expect("Could not set engine function");

        lua_engine
            .set("apply_force_2d", apply_force_2d)
            .expect("Could not set engine function");
        lua_engine
            .set("apply_move_2d", apply_move_2d)
            .expect("Could not set engine function");
        lua_engine
            .set("apply_impulse_2d", apply_impulse_2d)
            .expect("Could not set engine function");
        lua_engine
            .set("apply_masks_and_layers", apply_masks_and_layers)
            .expect("Could not set engine function");
        lua_engine
            .set("set_velocity_2d", set_velocity_2d)
            .expect("Could not set engine function");
        lua_engine
            .set("get_velocity_2d", get_velocity_2d)
            .expect("Could not set engine function");
        lua_engine
            .set("get_position_2d", get_position_2d)
            .expect("Could not set engine function");

        lua_engine
            .set("set_state", set_state)
            .expect("Could not set engine function");
        lua_engine
            .set("create_body", create_body)
            .expect("Could not set engine function");

        lua_engine
            .set("damage", damage)
            .expect("Could not set engine function");
        lua_engine
            .set("get_health", get_health)
            .expect("Could not set engine function");

        lua_engine
            .set("configure_camera", configure_camera)
            .expect("Could not set engine function");

        let now_ns = self
            .lua_context
            .lua
            .create_function(|_, ()| {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                Ok(now as u64) // or u128 if needed
            })
            .expect("Could not create function");
        lua_engine
            .set("now_ns", now_ns)
            .expect("Could not set engine function");

        self.lua_context
            .lua
            .globals()
            .set("engine", lua_engine)
            .expect("Could not define global engine");

        let config: mlua::Table = self
            .lua_context
            .get_function("ENGINE_load")
            .call::<mlua::Table>({})
            .expect("Unable to load initial assets.");

        let assets = config
            .get::<mlua::Table>("assets")
            .unwrap_or_else(|_| self.lua_context.create_table());
        for asset in assets.sequence_values::<String>() {
            let asset = asset.unwrap_or("".to_string());
            if asset != "" {
                let _ = self.get_texture(&asset);
            };
        }
    }

    fn call_lua_keyboard_input(&self, key: KeyCode, is_pressed: bool) {
        let _ = self
            .lua_context
            .get_function("ENGINE_input_event")
            .call::<()>((
                keycode_to_str(key),
                is_pressed,
                self.screen_to_world(self.mouse_pos),
            ));
    }

    fn call_lua_mouse_button_input(&self, button: MouseButton, is_pressed: bool) {
        let _ = self
            .lua_context
            .get_function("ENGINE_input_event")
            .call::<()>((
                mousebutton_to_str(button),
                is_pressed,
                self.screen_to_world(self.mouse_pos),
            ));
    }
}

impl ApplicationHandler<Graphics3D> for Engine {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = self.last_frame.elapsed();

        if self.fps_specified {
            let target = self.last_frame + self.target_rate.unwrap_or_default();
            if now < target {
                event_loop.set_control_flow(ControlFlow::WaitUntil(target));
                return;
            }
        }

        self.last_frame = Instant::now();
        let _ = self.update(dt);

        let graphics = match &mut self.graphics {
            Some(canvas) => canvas,
            None => return,
        };
        let _ = graphics.render(&self.world);

        self.count += 1;
        if self.count > SAFETY_MAX_FOR_DEV {
            event_loop.exit()
        }

        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes =
            Window::default_attributes().with_inner_size(LogicalSize::new(self.width, self.height));
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        if self.dimensions == Dimensions::Three {
            let camera_3d = Camera3D::new(
                self.width,
                self.height,
                crate::camera_3d::camera_3d::CameraMode::Orthographic2D,
            );
            self.graphics = Some(Box::new(
                pollster::block_on(Graphics3D::new(window.clone(), camera_3d)).unwrap(),
            ));
        } else if self.dimensions == Dimensions::Two {
            let camera_2d = Camera2D::new(self.width, self.height);
            self.graphics = Some(Box::new(
                pollster::block_on(Graphics2D::new(window.clone(), camera_2d)).unwrap(),
            ));
        }

        self.window = Some(window);

        self.setup();
        return;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.width = size.width;
                self.height = size.height;
                let graphics = match &mut self.graphics {
                    Some(canvas) => canvas,
                    None => return,
                };
                graphics.resize(size.width, size.height)
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button,
            } => {
                self.call_lua_mouse_button_input(button, state.is_pressed());
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Update mouse position
                self.mouse_pos = [position.x as f32, position.y as f32];
            }
            WindowEvent::RedrawRequested => {
                let graphics = match &mut self.graphics {
                    Some(canvas) => canvas,
                    None => return,
                };
                // this is the only place we want to call graphics.render()
                // any other situation should use self.redraw();
                let _ = graphics.render(&self.world);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                match (code, state.is_pressed()) {
                    (KeyCode::Escape, true) => event_loop.exit(),
                    _ => {}
                };
                self.call_lua_keyboard_input(code, state.is_pressed());
            }
            _ => {}
        }

        if self.camera_mode == CameraOption::Independent {
            let graphics = match &mut self.graphics {
                Some(canvas) => canvas,
                None => return,
            };
            graphics.process_camera_event(&event);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.cleanup()
    }
}

#[derive(Debug)]
struct LuaCameraKeyBinding {
    key: String,
    action: String,
}

#[derive(Debug)]
struct LuaCameraConfig {
    mode: String,
    speed: f32,
    locked: bool,
    keys: Vec<LuaCameraKeyBinding>,
}

impl LuaCameraConfig {
    fn from_lua_table(table: Table) -> Result<Self> {
        let mode: String = table.get("mode")?;
        let speed: f32 = table.get("speed")?;
        let locked: bool = table.get("locked")?;

        let keys_table: Table = table.get("keys")?;
        let mut keys = Vec::new();

        for pair in keys_table.sequence_values::<Table>() {
            let key_binding_table = pair?;
            let key: String = key_binding_table.get("key")?;
            let action: String = key_binding_table.get("action")?;

            keys.push(LuaCameraKeyBinding { key, action });
        }

        Ok(LuaCameraConfig {
            mode,
            speed,
            locked,
            keys,
        })
    }
    fn parse_keycode(s: &str) -> Result<KeyCode> {
        use KeyCode::*;
        let code = match s {
            "W" => KeyW,
            "A" => KeyA,
            "S" => KeyS,
            "D" => KeyD,
            "Q" => KeyQ,
            "E" => KeyE,
            "Up" => ArrowUp,
            "Down" => ArrowDown,
            "Left" => ArrowLeft,
            "Right" => ArrowRight,
            other => return Err(mlua::Error::RuntimeError(format!("Unknown key: {}", other))),
        };
        Ok(code)
    }

    fn parse_camera_action(s: &str) -> Result<CameraAction> {
        use CameraAction::*;
        let action = match s {
            "MoveForward" => MoveForward,
            "MoveBackward" => MoveBackward,
            "MoveLeft" => MoveLeft,
            "MoveRight" => MoveRight,
            "MoveUp" => MoveUp,
            "MoveDown" => MoveDown,
            "YawLeft" => YawLeft,
            "YawRight" => YawRight,
            "PitchUp" => PitchUp,
            "PitchDown" => PitchDown,
            "RollLeft" => RollLeft,
            "RollRight" => RollRight,
            other => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Unknown camera action: {}",
                    other
                )))
            }
        };
        Ok(action)
    }
}

fn mousebutton_to_str(button: MouseButton) -> Option<&'static str> {
    use MouseButton::*;
    Some(match button {
        Left => "mouseleft",
        Right => "mouseright",
        Middle => "mousemiddle",
        _ => return None,
    })
}

fn keycode_to_str(key: KeyCode) -> Option<&'static str> {
    use winit::keyboard::KeyCode::*;
    Some(match key {
        KeyW => "w",
        KeyA => "a",
        KeyS => "s",
        KeyD => "d",
        ArrowUp => "up",
        ArrowDown => "down",
        ArrowLeft => "left",
        ArrowRight => "right",
        Space => "space",
        Enter => "enter",
        Escape => "escape",
        KeyZ => "z",
        KeyX => "x",
        KeyC => "c",
        KeyV => "v",
        Digit0 => "0",
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Digit5 => "5",
        Digit6 => "6",
        Digit7 => "7",
        Digit8 => "8",
        Digit9 => "9",
        KeyQ => "q",
        KeyE => "e",
        KeyR => "r",
        KeyF => "f",
        KeyT => "t",
        KeyY => "y",
        KeyU => "u",
        KeyI => "i",
        KeyO => "o",
        KeyP => "p",
        KeyB => "b",
        KeyN => "n",
        KeyM => "m",
        _ => return None, // Unknown or unhandled key
    })
}

fn random_color() -> wgpu::Color {
    return wgpu::Color {
        r: rand::random(),
        b: rand::random(),
        g: rand::random(),
        a: rand::random(),
    };
}
