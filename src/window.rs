
use std::thread;
use std::sync::Arc;

use crossbeam::queue::SegQueue;

#[allow(unused_imports)]
use glium::{
    glutin,
    glutin::dpi,
    glutin::{Event, WindowEvent, DeviceEvent, KeyboardInput, VirtualKeyCode, ModifiersState},
    texture::{UnsignedTexture2d, buffer_texture::{BufferTexture, BufferTextureType}},
    draw_parameters::DrawParameters,
    Surface,
    Display,
    VertexBuffer,
    program::{Program, ProgramCreationInput},
    index::{self, IndexBuffer},
    backend::Facade,
};

/// OS-specific (conditional compilation) window configuration.
trait WindowBuilderOsSpecific: Sized {
    fn os_specific_window_configure(self) -> Self;
}

#[cfg(target_os = "macos")]
impl WindowBuilderOsSpecific for glutin::WindowBuilder {
    fn os_specific_window_configure(self) -> Self {
        use glium::backend::glutin::glutin::os::macos::WindowBuilderExt;

        self
            .with_movable_by_window_background(true)
    }
}

#[cfg(not(target_os = "macos"))]
impl WindowBuilderOsSpecific for glutin::WindowBuilder {
    fn os_specific_window_configure(self) -> Self {
        self
    }
}

/// Our vertex type.
#[derive(Copy, Clone)]
#[repr(C)]
struct Vertex { a_pos: [f32; 2] }

glium::implement_vertex!(Vertex, a_pos);

/// Simplified macro for creating our vertex array.
macro_rules! vertex_arr {
    [$( ($x:expr, $y:expr) ),*$(,)?] => {
        [$( Vertex { a_pos: [$x as f32, $y as f32] }, )*]
    }
}

/// Instruction to paint a single pixel.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Paint {
    pub x: usize,
    pub y: usize,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Open a software rendering window.
///
/// This will take over the current thread (which should be the main thread) until the window
/// closes, because some platforms require the window to be created in the main thread.
/// It will call the provided closure in its own thread, with a queue that can be sent
/// draw instructions.
pub fn open_window(
    x_size: usize,
    y_size: usize,
    draw_thread: impl FnOnce(Arc<SegQueue<Paint>>) + Send + 'static,
) {
    // reference-counted queue for painting
    let paint_queue_0 = Arc::new(SegQueue::new());
    let paint_queue_1 = paint_queue_0.clone();

    // spawn the drawing code in its own thread
    // (capture one of the queues for painting)
    thread::spawn(move || draw_thread(paint_queue_1));

    // create context
    let mut events_loop: glutin::EventsLoop = glutin::EventsLoop::new();
    let display: Display = {
        let wb = glutin::WindowBuilder::new()
            .with_dimensions(dpi::LogicalSize::new(x_size as _, y_size as _))
            .with_decorations(true)
            .with_transparency(true)
            .with_resizable(false)
            .os_specific_window_configure()
            .with_title("software rendering");
        let cb = glutin::ContextBuilder::new()
            .with_vsync(true);
        Display::new(wb, cb, &events_loop)
            .expect("display creation failure")
    };

    debug!("supported GLSL versions: {:?}", display.get_context().get_supported_glsl_version());

    // geometry to cover entire screen
    let vertex_buf: VertexBuffer<Vertex> = VertexBuffer::new(
        &display,
        &vertex_arr![
            (0, 0),
            (0, 1),
            (1, 1),
            (1, 0),
        ],
    ).expect("failed to create vertex buffer");

    let index_buf: IndexBuffer<u8> = IndexBuffer::new(
        &display,
        index::PrimitiveType::TriangleStrip,
        &[1, 2, 0, 3],
    ).expect("failed to create index buffer");

    // glsl program
    let program: Program = Program::from_source(
        &display,
        r###"

#version 410

in vec2 a_pos;

out vec2 v_pos;
out vec2 v_tex;

void main() {
    v_pos = (a_pos - vec2(0.5)) * 2.0;
    v_tex = a_pos;
    gl_Position = vec4(v_pos, 0.5, 1.0);
}

        "###,
        r###"

#version 410

uniform int x_size;
uniform int y_size;
uniform usamplerBuffer canvas_buf;

in vec2 v_pos;
in vec2 v_tex;

out vec4 f_col;

void main() {
    // background
    f_col = vec4(0.5);

    // compute our canvas integer coordinates
    uvec2 tex_xy = uvec2(v_tex * vec2(uvec2(x_size, y_size)));
    int index = int(tex_xy.y * x_size + tex_xy.x);

    // retrieve the painted pixel
    uvec4 painted_256 = texelFetch(canvas_buf, index);
    vec4 painted = vec4(painted_256) / 256.0;

    // mix it in, by its alpha
    f_col = mix(f_col, painted, painted.a);
}

        "###,
        None,
    ).expect("failed to create glsl program");

    // buffer to store the pixels
    // memory-mapped between CPU and GPU
    let mut canvas_buf_tex: BufferTexture<[u8; 4]> = {
        let num_zeroes: usize = x_size * y_size;
        let mut zeroes: Vec<[u8; 4]> = Vec::with_capacity(num_zeroes);
        for _ in 0..num_zeroes {
            zeroes.push([0x00, 0x00, 0x00, 0x00]);
        }

        BufferTexture::dynamic(
            &display,
            &zeroes,
            BufferTextureType::Unsigned,
        ).expect("error creating buffer texture")
    };

    // window loop
    let mut open = true;
    while open {
        // render
        {
            let uniforms = glium::uniform! {
                x_size: x_size as i32,
                y_size: y_size as i32,
                canvas_buf: &canvas_buf_tex
            };

            let draw_params = DrawParameters::default();

            let mut frame = display.draw();
            frame.clear_color_and_depth(
                (1.0, 1.0, 1.0, 0.0),
                1.0,
            );
            frame.draw(
                &vertex_buf,
                &index_buf,
                &program,
                &uniforms,
                &draw_params,
            ).expect("draw call failed");
            frame.finish()
                .expect("failed to swap frame buffers");
        }

        // apply instructions from the paint queue
        if !paint_queue_0.is_empty() {
            let mut canvas_mmap = canvas_buf_tex.map_write();

            while let Ok(Paint {
                             x,
                             y,
                             r,
                             g,
                             b,
                             a,
                         }) = paint_queue_0.pop() {

                let rgba = [r, g, b, a];
                let i: usize = y * x_size + x;

                canvas_mmap.set(i, rgba);

            }
        }

        // poll
        events_loop.poll_events(|event| {
            match event {

                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    // window "X'd out"
                    open = false;
                },

                Event::DeviceEvent { event: DeviceEvent::Key(
                    KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::W),
                        modifiers: ModifiersState { logo: true, .. },
                        ..
                    }
                ), .. } => {
                    // cmd+w
                    open = false;
                },

                Event::WindowEvent { event: WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::W),
                        modifiers: ModifiersState { logo: true, .. },
                        ..
                    },
                    ..
                }, .. } => {
                    // cmd+w
                    open = false;
                }

                Event::DeviceEvent { event: DeviceEvent::Key(
                    KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::W),
                        modifiers: ModifiersState { ctrl: true, .. },
                        ..
                    }
                ), .. } => {
                    // ctrl+w
                    open = false;
                },

                Event::WindowEvent { event: WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::W),
                        modifiers: ModifiersState { ctrl: true, .. },
                        ..
                    },
                    ..
                }, .. } => {
                    // ctrl+w
                    open = false;
                }

                _ => ()

            }
        });
    }

    trace!("closing window");
}