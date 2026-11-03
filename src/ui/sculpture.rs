//! Small SDF sculptures: Surface Nets modeling, three-d PBR, egui Glow composition.
//! GPU state stays on the GL thread; callbacks capture only plain frame parameters.
use egui::{Color32, Rect, Ui};
use fast_surface_nets::{SurfaceNetsBuffer, ndshape::ConstShape, ndshape::ConstShape3u32};
use std::{cell::RefCell, sync::Arc};
use three_d::{
    AmbientLight, Blend, Camera, ClearState, ColorTexture, Context, CopyEffect, CoreError,
    CpuMaterial, CpuMesh, DepthTexture2D, DirectionalLight, GeometryFunction, Gm, Indices,
    InnerSpace, Interpolation, Light, LightingModel, Mat4, Mesh, NormalDistributionFunction,
    PhysicalMaterial, Positions, RenderTarget, ScissorBox, Srgba, Texture2D, Vec3, Viewport,
    Wrapping, WriteMask, radians, vec3,
};

#[derive(Clone, Copy)]
pub struct Sculpture {
    pub role: usize,
    pub color: Color32,
    pub time: f32,
}

type Grid = ConstShape3u32<40, 40, 40>;
type Object = Gm<Mesh, PhysicalMaterial>;

thread_local! {
    static STUDIO: RefCell<Option<Studio>> = const { RefCell::new(None) };
}

/// Release owned models, textures and shader cache while the host GL is current.
pub fn destroy() {
    STUDIO.with(|studio| studio.borrow_mut().take());
}

pub fn paint(ui: &Ui, rect: Rect, sculpture: Sculpture) {
    if !ui.is_rect_visible(rect) || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let callback = eframe::egui_glow::CallbackFn::new(move |info, painter| {
        STUDIO.with(|slot| {
            let mut studio = slot.borrow_mut();
            if studio
                .as_ref()
                .is_none_or(|s| !Arc::ptr_eq(&*s.context, painter.gl()))
            {
                *studio = Studio::new(painter.gl().clone()).ok();
            }
            if let Some(studio) = studio.as_mut() {
                studio.paint(info, painter, sculpture);
            }
        });
    });
    ui.painter().add(egui::PaintCallback {
        rect,
        callback: Arc::new(callback),
    });
}

struct Studio {
    context: Context,
    sculptures: Vec<Object>,
    pedestal: Object,
    keys: Vec<DirectionalLight>,
    lights: [DirectionalLight; 2],
    ambient: AmbientLight,
    color: Texture2D,
    depth: DepthTexture2D,
}

impl Studio {
    fn new(gl: Arc<eframe::glow::Context>) -> Result<Self, CoreError> {
        let context = Context::from_gl_context(gl)?;
        let sculptures: Vec<_> = (0..4)
            .map(|role| {
                let (roughness, metallic) =
                    [(0.30, 0.15), (0.22, 0.65), (0.24, 0.40), (0.42, 0.20)][role];
                let mut sculpture = model(&context, &sculpt(role), roughness, metallic);
                sculpture.set_transformation(
                    Mat4::from_translation(vec3(0.0, if role == 3 { -0.20 } else { 0.0 }, 0.0))
                        * Mat4::from_angle_y(radians(-0.22)),
                );
                sculpture
            })
            .collect();
        let pedestal = model(&context, &pedestal_mesh(), 0.7, 0.65);
        let keys = sculptures
            .iter()
            .map(|sculpture| {
                let mut key = DirectionalLight::new(
                    &context,
                    3.5,
                    Srgba::new_opaque(255, 235, 215),
                    vec3(-1.0, -1.0, -2.0),
                );
                key.generate_shadow_map(512, [&sculpture.geometry, &pedestal.geometry])
                    .expect("valid sculpture shadow map");
                key
            })
            .collect();
        let lights = studio_lights(&context);
        let ambient = AmbientLight::new(&context, 0.35, Srgba::WHITE);
        let color = Texture2D::new_empty::<[u8; 4]>(
            &context,
            384,
            384,
            Interpolation::Linear,
            Interpolation::Linear,
            None,
            Wrapping::ClampToEdge,
            Wrapping::ClampToEdge,
        );
        let depth = DepthTexture2D::new::<f32>(
            &context,
            384,
            384,
            Wrapping::ClampToEdge,
            Wrapping::ClampToEdge,
        );
        Ok(Self {
            context,
            sculptures,
            pedestal,
            keys,
            lights,
            ambient,
            color,
            depth,
        })
    }

    fn paint(
        &mut self,
        info: egui::PaintCallbackInfo,
        painter: &eframe::egui_glow::Painter,
        frame: Sculpture,
    ) {
        let Sculpture { role, color, time } = frame;
        let sculpture = &mut self.sculptures[role.min(3)];
        sculpture.material.albedo = Srgba::new_opaque(color.r(), color.g(), color.b());
        let mut camera = camera(Viewport::new_at_origo(384, 384));
        camera.set_view(
            vec3(3.0 + time.sin() * 0.30, 2.1, 6.0),
            vec3(0.0, -0.05, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        let lights: [&dyn Light; 4] = [
            &self.ambient,
            &self.keys[role.min(3)],
            &self.lights[0],
            &self.lights[1],
        ];
        RenderTarget::new(
            self.color.as_color_target(None),
            self.depth.as_depth_target(),
        )
        .clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 0.0, 1.0))
        .render(&camera, [sculpture as &Object, &self.pedestal], &lights);
        self.composite(info, painter);
    }

    fn composite(&self, info: egui::PaintCallbackInfo, painter: &eframe::egui_glow::Painter) {
        let v = info.viewport_in_pixels();
        let c = info.clip_rect_in_pixels();
        let viewport = Viewport {
            x: v.left_px,
            y: v.from_bottom_px,
            width: v.width_px.max(0).cast_unsigned(),
            height: v.height_px.max(0).cast_unsigned(),
        };
        let clip = ScissorBox {
            x: c.left_px,
            y: c.from_bottom_px,
            width: c.width_px.max(0).cast_unsigned(),
            height: c.height_px.max(0).cast_unsigned(),
        };
        if clip.width == 0 || clip.height == 0 || viewport.width == 0 || viewport.height == 0 {
            return;
        }
        let [width, height] = info.screen_size_px;
        let target = painter.intermediate_fbo().map_or_else(
            || RenderTarget::screen(&self.context, width, height),
            |fbo| RenderTarget::from_framebuffer(&self.context, width, height, fbo),
        );
        let effect = CopyEffect {
            write_mask: WriteMask::COLOR,
            blend: Blend::TRANSPARENCY,
        };
        target.apply_screen_effect_partially(
            clip,
            &effect,
            camera(viewport),
            &[],
            Some(ColorTexture::Single(&self.color)),
            None,
        );
        // This framebuffer belongs to egui. Never let three-d delete it on drop.
        target.into_framebuffer();
    }
}

fn camera(viewport: Viewport) -> Camera {
    Camera::new_orthographic(
        viewport,
        vec3(3.0, 2.1, 6.0),
        vec3(0.0, -0.05, 0.0),
        vec3(0.0, 1.0, 0.0),
        0.43,
        0.1,
        30.0,
    )
}

fn studio_lights(context: &Context) -> [DirectionalLight; 2] {
    [
        DirectionalLight::new(
            context,
            1.4,
            Srgba::new_opaque(155, 190, 255),
            vec3(1.0, -0.3, 1.0),
        ),
        DirectionalLight::new(
            context,
            1.0,
            Srgba::new_opaque(230, 175, 255),
            vec3(-0.2, 1.0, -1.0),
        ),
    ]
}

fn model(context: &Context, mesh: &CpuMesh, roughness: f32, metallic: f32) -> Object {
    Gm::new(
        Mesh::new(context, mesh),
        PhysicalMaterial::new_opaque(
            context,
            &CpuMaterial {
                albedo: Srgba::new_opaque(100, 95, 135),
                roughness,
                metallic,
                lighting_model: LightingModel::Cook(
                    NormalDistributionFunction::TrowbridgeReitzGGX,
                    GeometryFunction::SmithSchlickGGX,
                ),
                ..Default::default()
            },
        ),
    )
}

fn pedestal_mesh() -> CpuMesh {
    sculpt(4)
}

/// Fixed-size, closed fields are meshed once per GL context, never per frame.
fn sculpt(role: usize) -> CpuMesh {
    let field: Vec<f32> = (0..Grid::SIZE)
        .map(|i| {
            let p = Grid::delinearize(i).map(|v| v as f32 * (3.0 / 39.0) - 1.5);
            distance(role, vec3(p[0], p[1], p[2]))
        })
        .collect();
    let mut buffer = SurfaceNetsBuffer::default();
    fast_surface_nets::surface_nets(&field, &Grid {}, [0; 3], [39; 3], &mut buffer);
    let positions = buffer
        .positions
        .iter()
        .map(|p| vec3(p[0], p[1], p[2]) * (3.0 / 39.0) - vec3(1.5, 1.5, 1.5))
        .collect();
    let normals = buffer
        .normals
        .iter()
        .map(|n| vec3(n[0], n[1], n[2]).normalize())
        .collect();
    CpuMesh {
        positions: Positions::F32(positions),
        normals: Some(normals),
        indices: Indices::U32(buffer.indices),
        ..Default::default()
    }
}

fn distance(role: usize, p: Vec3) -> f32 {
    match role {
        // Idea: six softly fused petals around a central seed.
        0 => bloom(p),
        // Challenge: a cut mineral with softened corners and broad facets.
        1 => {
            ((p.x * p.x + 0.002).sqrt() + (p.y * p.y + 0.002).sqrt() + (p.z * p.z + 0.002).sqrt()
                - 0.84)
                * 0.577
                - 0.04
        }
        // Reply: a continuous twisted loop, a visible opening from the camera.
        2 => torus(p),
        // Brief: three rounded tablets.
        3 => tablets(p),
        _ => pedestal_distance(p),
    }
}

fn pedestal_distance(p: Vec3) -> f32 {
    let radial = (p.x * p.x + p.z * p.z).sqrt() - 0.82;
    let vertical = (p.y + 0.97).abs() - 0.055;
    (radial.max(0.0).powi(2) + vertical.max(0.0).powi(2)).sqrt() + radial.max(vertical).min(0.0)
        - 0.055
}

fn bloom(p: Vec3) -> f32 {
    let mut d = p.magnitude() - 0.34;
    for i in 0..6 {
        let angle = i as f32 * std::f32::consts::TAU / 6.0;
        let center = vec3(angle.cos() * 0.52, angle.sin() * 0.52, 0.0);
        let petal = (p - center).magnitude() - 0.30;
        let h = (0.5 + 0.5 * (petal - d) / 0.17).clamp(0.0, 1.0);
        d = petal * (1.0 - h) + d * h - 0.17 * h * (1.0 - h);
    }
    d
}

fn torus(p: Vec3) -> f32 {
    let angle = p.y * 0.55;
    let x = p.x * angle.cos() - p.z * angle.sin();
    let z = p.x * angle.sin() + p.z * angle.cos();
    let radius = (x * x + p.y * p.y).sqrt() - 0.58;
    (radius * radius + z * z * 1.25).sqrt() - 0.24
}

fn tablets(p: Vec3) -> f32 {
    let mut distance = f32::MAX;
    for layer in [-1.0, 0.0, 1.0] {
        let q = vec3(
            p.x.abs() - 0.55,
            (p.y - layer * 0.44).abs() - 0.10,
            p.z.abs() - 0.40,
        );
        let outside = vec3(q.x.max(0.0), q.y.max(0.0), q.z.max(0.0));
        distance = distance.min(outside.magnitude() + q.x.max(q.y.max(q.z)).min(0.0) - 0.12);
    }
    distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_framing_keeps_sculptures_large_and_inside_the_tile() {
        let camera = camera(Viewport::new_at_origo(384, 384));
        let view_projection = camera.projection() * camera.view();
        let top = view_projection * three_d::vec4(0.0, 0.9, 0.0, 1.0);
        let bottom = view_projection * three_d::vec4(0.0, -1.1, 0.0, 1.0);
        let span = top.y / top.w - bottom.y / bottom.w;
        assert!(
            span > 1.0 && span < 1.6,
            "sculpture must fill most of the tile: {span}"
        );
        assert!(top.y.abs() < top.w && bottom.y.abs() < bottom.w);
    }

    fn assert_closed(mesh: &CpuMesh) {
        let Indices::U32(indices) = &mesh.indices else {
            panic!("indexed mesh")
        };
        let mut edges = std::collections::HashMap::new();
        for triangle in indices.as_chunks::<3>().0 {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|count| *count == 2),
            "every surface edge needs two incident faces"
        );
    }

    #[test]
    fn sculptures_have_closed_finite_indexed_geometry_and_distinct_silhouettes() {
        assert_closed(&pedestal_mesh());
        let mut counts = Vec::new();
        for role in 0..4 {
            let mesh = sculpt(role);
            mesh.validate().unwrap();
            assert_closed(&mesh);
            let Positions::F32(positions) = mesh.positions else {
                panic!("f32 geometry")
            };
            assert!(positions.len() > 500);
            assert!(
                positions
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite())
            );
            assert!(
                mesh.normals
                    .unwrap()
                    .iter()
                    .all(|n| (n.magnitude() - 1.0).abs() < 0.001)
            );
            counts.push(positions.len());
            for p in [
                vec3(1.5, 0.0, 0.0),
                vec3(0.0, 1.5, 0.0),
                vec3(0.0, 0.0, 1.5),
            ] {
                assert!(
                    distance(role, p) > 0.0,
                    "field must close before grid boundary"
                );
            }
        }
        counts.sort_unstable();
        counts.dedup();
        assert_eq!(counts.len(), 4);
        assert!(
            distance(2, vec3(0.0, 0.0, 0.0)) > 0.0,
            "reply loop retains its opening"
        );
    }
}
