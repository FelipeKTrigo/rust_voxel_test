use wgpu::util::DeviceExt;
use crate::renderer::mesh::VoxelVertex;

/// Malha dinâmica (reconstruída por quadro) — usada pelos barcos.
#[derive(Default)]
pub struct DynMesh {
    buffers: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
}

impl DynMesh {
    pub fn update(&mut self, device: &wgpu::Device, vertices: &[VoxelVertex], indices: &[u32]) {
        if indices.is_empty() {
            self.buffers = None;
            return;
        }
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dyn VB"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dyn IB"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.buffers = Some((vb, ib, indices.len() as u32));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if let Some((vb, ib, n)) = &self.buffers {
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
        }
    }
}
