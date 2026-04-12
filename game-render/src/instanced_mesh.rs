use std::cell::Cell;
use wgpu::util::DeviceExt;

/// Shared GPU state for instanced mesh renderers (player).
///
/// Owns vertex/index/instance buffers and scene pipeline.
/// Provides draw() and update_instances().
pub struct InstancedMeshRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_count: Cell<u32>,
}

impl InstancedMeshRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
        _shadow_pipeline: Option<wgpu::RenderPipeline>,
        vertices: &[u8],
        indices: &[u32],
        instance_size: usize,
        max_instances: usize,
        initial_instances: &[u8],
        label: &str,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Verts", label)),
            contents: vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_count = indices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Idx", label)),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{} Instances", label)),
            size: (max_instances * instance_size) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let count = if instance_size > 0 {
            (initial_instances.len() / instance_size).min(max_instances) as u32
        } else {
            0
        };
        if !initial_instances.is_empty() {
            let byte_len = count as usize * instance_size;
            queue.write_buffer(&instance_buffer, 0, &initial_instances[..byte_len]);
        }

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            instance_buffer,
            instance_count: Cell::new(count),
        }
    }

    /// Update the instance buffer contents and count. Works through &self.
    pub fn update_instances(&self, queue: &wgpu::Queue, data: &[u8], count: u32) {
        self.instance_count.set(count);
        if !data.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, data);
        }
    }

    pub fn instance_buffer(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }

    pub fn instance_count(&self) -> u32 {
        self.instance_count.get()
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
    ) {
        let count = self.instance_count.get();
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, shadow_bg, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..count);
    }

}
