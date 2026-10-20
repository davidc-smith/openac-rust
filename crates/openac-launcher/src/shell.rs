//! The launcher's window: a winit window, a wgpu surface, and egui drawn
//! into it. Everything the launcher shows comes from one [`LauncherUi`];
//! this module only turns window events into egui input and egui output
//! into frames.
//!
//! The window repaints when egui asks (input, animation, a requested delay)
//! and when a background worker wakes it through the [`Waker`]; otherwise
//! the event loop sleeps.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// Wakes the window from another thread so it draws again.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What the window draws and asks.
pub trait LauncherUi {
    /// Draws one frame. `window_active` is whether the window has focus.
    fn update(&mut self, ui: &mut egui::Ui, window_active: bool);

    /// The window's close button was pressed; true lets the window close.
    fn close_requested(&mut self) -> bool {
        true
    }

    /// The window is closing for good: stop background work.
    fn on_exit(&mut self) {}

    /// A frame finished; `capture` reads its pixels when the UI asked for a
    /// screenshot (see [`LauncherUi::wants_screenshot`]).
    fn frame_captured(&mut self, _image: CapturedFrame) {}

    /// Whether the next frame should be read back.
    fn wants_screenshot(&mut self) -> bool {
        false
    }

    /// Whether the UI wants the window closed now.
    fn wants_exit(&mut self) -> bool {
        false
    }
}

/// A frame read back from the GPU: RGBA, top row first.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes per pixel, red first.
    pub rgba: Vec<u8>,
}

/// How the window opens.
#[derive(Debug, Clone)]
pub struct ShellOptions {
    /// The title bar text.
    pub title: String,
    /// The first size, in logical pixels.
    pub size: (f64, f64),
    /// The smallest size, in logical pixels.
    pub min_size: (f64, f64),
    /// The title bar and task bar image, if any.
    pub icon: Option<WindowIcon>,
}

/// A window icon: 8-bit RGBA, top row first.
#[derive(Debug, Clone)]
pub struct WindowIcon {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes per pixel, red first.
    pub rgba: Vec<u8>,
}

/// Why the window could not run.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// The event loop could not be created or failed.
    #[error("the window system failed: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    /// The window could not be created.
    #[error("the window could not be created: {0}")]
    Window(#[from] winit::error::OsError),
    /// No GPU surface for the window.
    #[error("the window has no GPU surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    /// No GPU adapter can present to the window.
    #[error("no GPU adapter can draw this window: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    /// The GPU device could not be opened.
    #[error("the GPU device could not be opened: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    /// The surface has no format the adapter supports.
    #[error("the window surface is not supported by the GPU adapter")]
    SurfaceUnsupported,
}

/// Runs the window on this thread (the main thread) until it closes.
/// `make` builds the UI once the egui context and the waker exist.
pub fn run<U: LauncherUi>(
    options: ShellOptions,
    make: impl FnOnce(egui::Context, Waker) -> U,
) -> Result<(), ShellError> {
    let event_loop = EventLoop::<()>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy: EventLoopProxy<()> = event_loop.create_proxy();
    let waker: Waker = Arc::new(move || {
        // A closed loop has nobody to wake.
        let _ = proxy.send_event(());
    });
    let ctx = egui::Context::default();
    let ui = make(ctx.clone(), waker);
    let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
        event_loop.owned_display_handle(),
    ));
    // DX12 shaders compile with FXC, which ships with Windows (as ac-render does): the default
    // would load a dxcompiler.dll found on PATH, which is neither OS nor driver.
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    let mut app = ShellApp {
        options,
        instance: wgpu::Instance::new(descriptor),
        ctx,
        ui,
        window: None,
        error: None,
        next_repaint: None,
        clipboard: arboard::Clipboard::new().ok(),
        modifiers: ModifiersState::empty(),
    };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct ShellApp<U: LauncherUi> {
    options: ShellOptions,
    instance: wgpu::Instance,
    ctx: egui::Context,
    ui: U,
    window: Option<OpenWindow>,
    error: Option<ShellError>,
    next_repaint: Option<Instant>,
    clipboard: Option<arboard::Clipboard>,
    modifiers: ModifiersState,
}

struct OpenWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
    state: egui_winit::State,
    active: bool,
}

impl OpenWindow {
    fn open(
        event_loop: &ActiveEventLoop,
        instance: &wgpu::Instance,
        ctx: &egui::Context,
        options: &ShellOptions,
    ) -> Result<Self, ShellError> {
        let attributes = Window::default_attributes()
            .with_title(options.title.clone())
            .with_inner_size(LogicalSize::new(options.size.0, options.size.1))
            .with_min_inner_size(LogicalSize::new(options.min_size.0, options.min_size.1))
            .with_window_icon(options.icon.as_ref().and_then(|icon| {
                // A platform without window icons (Wayland) or an image it
                // refuses leaves the window without one: an icon is never
                // worth failing the launcher.
                winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height)
                    .map_err(|error| eprintln!("window icon: {error}"))
                    .ok()
            }));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let surface = instance.create_surface(Arc::clone(&window))?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("launcher device"),
            ..Default::default()
        }))?;
        // The default handler panics; a launcher drawing error is not worth a crash.
        device.on_uncaptured_error(Arc::new(|_error: wgpu::Error| {}));
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or(ShellError::SurfaceUnsupported)?;
        // egui writes gamma-space colours, so a non-sRGB format shows them as written.
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(format) = capabilities.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *format;
        }
        if capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        if size.width != 0 && size.height != 0 {
            surface.configure(&device, &config);
        }
        let renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            state,
            active: true,
        })
    }

    /// The next frame's image, or `None` when there is none this time (the
    /// surface is rebuilt or the window hidden; a redraw is asked for).
    fn acquire(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let size = self.window.inner_size();
                self.resize(size.width, size.height);
                self.window.request_redraw();
                None
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                self.window.request_redraw();
                None
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

impl<U: LauncherUi> ShellApp<U> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: ShellError) {
        self.error = Some(error);
        self.ui.on_exit();
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(open) = self.window.as_mut() else {
            return;
        };
        let raw = open.state.take_egui_input(&open.window);
        let active = open.active;
        let ui = &mut self.ui;
        let egui::FullOutput {
            platform_output,
            mut textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = self.ctx.run_ui(raw, |root| ui.update(root, active));
        // Text copied in the window goes to the system clipboard.
        for command in &platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => {
                    if let Some(clipboard) = self.clipboard.as_mut() {
                        let _ = clipboard.set_text(text.clone());
                    }
                }
                // A link (a known server's website) opens in the system's browser.
                egui::OutputCommand::OpenUrl(open) => {
                    open_link(&open.url);
                }
                _ => {}
            }
        }
        open.state
            .handle_platform_output(&open.window, platform_output);
        let jobs = self.ctx.tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [open.config.width, open.config.height],
            pixels_per_point,
        };
        // Every texture change is applied (or freed) whether or not this frame is presented.
        for (id, deltas) in textures_delta.set.drain() {
            for delta in &deltas {
                open.renderer
                    .update_texture(&open.device, &open.queue, id, delta);
            }
        }
        let freed: Vec<egui::TextureId> = textures_delta.free.drain().collect();
        self.next_repaint = None;
        if let Some(texture) = open.acquire() {
            self.paint(texture, &jobs, &screen);
        }
        let Some(open) = self.window.as_mut() else {
            return;
        };
        for id in &freed {
            open.renderer.free_texture(id);
        }
        let root = viewport_output.get(&egui::ViewportId::ROOT);
        // The probe script's window-size; the window's other commands are not used.
        for command in root.map_or(&[][..], |viewport| &viewport.commands[..]) {
            if let egui::ViewportCommand::InnerSize(size) = command {
                let _ = open
                    .window
                    .request_inner_size(LogicalSize::new(f64::from(size.x), f64::from(size.y)));
            }
        }
        let delay = root.map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        self.next_repaint = if delay == Duration::ZERO {
            open.window.request_redraw();
            None
        } else {
            Instant::now().checked_add(delay)
        };
        if self.ui.wants_exit() {
            self.ui.on_exit();
            self.window = None;
            event_loop.exit();
        }
    }

    fn paint(
        &mut self,
        texture: wgpu::SurfaceTexture,
        jobs: &[egui::ClippedPrimitive],
        screen: &egui_wgpu::ScreenDescriptor,
    ) {
        let Some(open) = self.window.as_mut() else {
            return;
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = open
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("launcher frame"),
            });
        let mut commands =
            open.renderer
                .update_buffers(&open.device, &open.queue, &mut encoder, jobs, screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("launcher pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.063,
                            g: 0.090,
                            b: 0.110,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            open.renderer.render(&mut pass, jobs, screen);
        }
        let capture =
            self.ui.wants_screenshot() && open.config.usage.contains(wgpu::TextureUsages::COPY_SRC);
        let readback = capture.then(|| {
            Readback::record(
                &open.device,
                &mut encoder,
                &texture.texture,
                open.config.width,
                open.config.height,
            )
        });
        commands.push(encoder.finish());
        open.queue.submit(commands);
        if let Some(readback) = readback
            && let Some(frame) = readback.read(&open.device, open.config.format)
        {
            self.ui.frame_captured(frame);
        }
        open.queue.present(texture);
    }
}

impl<U: LauncherUi> ApplicationHandler for ShellApp<U> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match OpenWindow::open(event_loop, &self.instance, &self.ctx, &self.options) {
            Ok(open) => {
                open.window.request_redraw();
                self.window = Some(open);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, (): ()) {
        if let Some(open) = &self.window {
            open.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(open) = self.window.as_mut() else {
            return;
        };
        match &event {
            WindowEvent::CloseRequested => {
                if self.ui.close_requested() {
                    self.ui.on_exit();
                    self.window = None;
                    event_loop.exit();
                }
                return;
            }
            WindowEvent::Resized(size) => open.resize(size.width, size.height),
            WindowEvent::Focused(focused) => open.active = *focused,
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event: key, .. }
                if key.state == ElementState::Pressed
                    && is_paste(&key.logical_key, self.modifiers) =>
            {
                // egui-winit pastes from its own clipboard; fill it from the system's first.
                if let Some(text) = self
                    .clipboard
                    .as_mut()
                    .and_then(|clipboard| clipboard.get_text().ok())
                {
                    open.state.set_clipboard_text(text);
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
                return;
            }
            _ => {}
        }
        let response = open.state.on_window_event(&open.window, &event);
        if response.repaint {
            open.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self.next_repaint {
            Some(at) if at <= Instant::now() => {
                self.next_repaint = None;
                if let Some(open) = &self.window {
                    open.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn is_paste(key: &Key, modifiers: ModifiersState) -> bool {
    let command = if cfg!(target_os = "macos") {
        modifiers.super_key()
    } else {
        modifiers.control_key()
    };
    match key {
        Key::Character(text) => command && text.eq_ignore_ascii_case("v"),
        Key::Named(NamedKey::Insert) => modifiers.shift_key(),
        Key::Named(NamedKey::Paste) => true,
        _ => false,
    }
}

/// A copy of the frame into a buffer the CPU can read.
struct Readback {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_row: u32,
}

impl Readback {
    fn record(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Self {
        let row = width * 4;
        let padded_row =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("launcher screenshot"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Self {
            buffer,
            width,
            height,
            padded_row,
        }
    }

    fn read(self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Option<CapturedFrame> {
        let slice = self.buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        receiver.recv().ok()?.ok()?;
        let data = slice.get_mapped_range().ok()?;
        let swap = matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut rgba = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * self.padded_row) as usize;
            let line = &data[start..start + (self.width * 4) as usize];
            for pixel in line.chunks_exact(4) {
                if swap {
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
                } else {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }
        }
        drop(data);
        self.buffer.unmap();
        Some(CapturedFrame {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}

/// Runs a future to completion on this thread (wgpu's adapter and device
/// requests are futures that finish without an executor on native).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker as TaskWaker};

    struct Unparker(std::thread::Thread);
    impl Wake for Unparker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = TaskWaker::from(Arc::new(Unparker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// Opens `folder` in the system's file manager (Explorer, Finder, or the
/// desktop's handler through `xdg-open`); false when it could not be
/// started. Explorer reports a failure exit code even when it opened the
/// folder, so starting it is what counts.
pub fn open_folder(folder: &std::path::Path) -> bool {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Opens an `http` or `https` address in the system's browser; false
/// when it is not one or could not be started.
pub fn open_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// The system clipboard's text, for the editors' Paste all; `None` when
/// there is no text or no clipboard.
pub fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
