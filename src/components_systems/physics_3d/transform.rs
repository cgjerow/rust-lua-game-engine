use std::collections::HashMap;

use crate::{components_systems::Entity, world::World};

#[derive(Debug, Clone)]
pub struct TransformComponent {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub size: [f32; 3],
}

pub fn transform_system_physics(_world: &mut World, _delta_time: f32) {
    /*
    let mut idled = Vec::new();
    let idle_threshold = 0.1; // velocity magnitude below which entity is considered idle

    for (entity, transform) in world.transforms_3d.iter_mut() {
        // Compute velocity magnitude before update (optional if you want previous velocity)
        let prev_speed = (transform.velocity[0].powi(2) + transform.velocity[1].powi(2)).sqrt();

        // Integrate acceleration into velocity
        transform.velocity[0] += transform.acceleration[0] * delta_time;
        transform.velocity[1] += transform.acceleration[1] * delta_time;

        // Apply velocity to position
        transform.position[0] += transform.velocity[0] * delta_time;
        transform.position[1] += transform.velocity[1] * delta_time;

        // Apply drag or friction (optional)
        let drag = 0.8;
        transform.velocity[0] *= drag;
        transform.velocity[1] *= drag;

        // Compute new speed after update
        let new_speed = (transform.velocity[0].powi(2) + transform.velocity[1].powi(2)).sqrt();

        // Detect idle transition: was moving, now slow enough to be idle
        if prev_speed > idle_threshold && new_speed <= idle_threshold {
            idled.push(entity.clone());
        }

        // Clear acceleration after use
        transform.acceleration = [0.0; 3];
    }
    */
}

pub fn transform_system_calculate_intended_position(
    _world: &World,
    _delta_time: f32,
) -> HashMap<Entity, TransformComponent> {
    let to_return = HashMap::new();
    /*
    for (id, before) in world.transforms_3d.iter() {
        let mut transform = before.clone();
        // Integrate acceleration into velocity
        transform.velocity[0] += transform.acceleration[0] * delta_time;
        transform.velocity[1] += transform.acceleration[1] * delta_time;

        // Clamp speed (optional, example max: 300.0)
        let speed = (transform.velocity[0].powi(2) + transform.velocity[1].powi(2)).sqrt();
        let max_speed = 30.0;
        if speed > max_speed {
            let scale = max_speed / speed;
            transform.velocity[0] *= scale;
            transform.velocity[1] *= scale;
        }

        // Apply velocity to position
        transform.position[0] += transform.velocity[0] * delta_time;
        transform.position[1] += transform.velocity[1] * delta_time;

        // Apply drag or friction (optional)
        let drag = 0.8;
        transform.velocity[0] *= drag;
        transform.velocity[1] *= drag;

        // Clear acceleration after use
        transform.acceleration = [0.0; 3];

        to_return.insert(id.clone(), transform);
    }
    */
    to_return
}

pub fn transform_system_add_acceleration(world: &mut World, id: Entity, dx: f32, dy: f32) {}

pub fn transform_system_redirect(
    world: &mut World,
    id: Entity,
    dx: f32,
    dy: f32,
    sep_x: f32,
    sep_y: f32,
    acceleration_mod: f32,
) {
}
