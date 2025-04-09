use cgmath::{InnerSpace, Point3, Vector2, Vector3};
use derive_new::new;
use smallvec::{SmallVec, smallvec_inline};

use crate::graphics::{Color, ModelVertex};

#[derive(Clone, new)]
pub struct NativeModelVertex {
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
    pub texture_coordinates: Vector2<f32>,
    pub texture_index: i32,
    pub color: Color,
    pub wind_affinity: f32,
    pub smoothing_groups: SmallVec<[i32; 3]>,
}

impl NativeModelVertex {
    pub const fn zeroed() -> NativeModelVertex {
        NativeModelVertex {
            position: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 0.0),
            texture_coordinates: Vector2::new(0.0, 0.0),
            texture_index: 0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.0),
            wind_affinity: 0.0,
            smoothing_groups: smallvec_inline![0; 3],
        }
    }

    fn into_model_vertex(self) -> ModelVertex {
        ModelVertex::new(
            self.position,
            self.normal,
            self.texture_coordinates,
            self.color,
            self.texture_index,
            self.wind_affinity,
        )
    }

    pub fn to_model_vertices(mut native_vertices: Vec<NativeModelVertex>) -> Vec<ModelVertex> {
        native_vertices.drain(..).map(|v| v.into_model_vertex()).collect()
    }

    pub fn calculate_normal(first_position: Point3<f32>, second_position: Point3<f32>, third_position: Point3<f32>) -> Vector3<f32> {
        let delta_position_1 = second_position - first_position;
        let delta_position_2 = third_position - first_position;
        delta_position_1.cross(delta_position_2).normalize()
    }
}
