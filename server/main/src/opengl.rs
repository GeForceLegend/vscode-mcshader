use std::ffi::{c_int, CStr, CString};
use std::ptr;

pub struct OpenGlContext {
    _ctx: glutin::Context<glutin::PossiblyCurrent>,
}

impl OpenGlContext {
    pub fn new() -> OpenGlContext {
        let events_loop = glutin::event_loop::EventLoop::new();
        let not_current_context = glutin::ContextBuilder::new()
            .build_headless(&*events_loop, glutin::dpi::PhysicalSize::new(1, 1))
            .unwrap();

        let context = unsafe { not_current_context.make_current().unwrap() };
        gl::load_with(|symbol| context.get_proc_address(symbol));

        OpenGlContext { _ctx: context }
    }

    pub fn validate_shader(&self, file_type: gl::types::GLenum, source: String) -> Option<String> {
        unsafe {
            let shader = gl::CreateShader(file_type);
            let c_str_frag = CString::new(source).unwrap();
            gl::ShaderSource(shader, 1, &c_str_frag.as_ptr(), ptr::null());
            gl::CompileShader(shader);

            // Check for shader compilation errors
            let mut success = gl::FALSE as i32;
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
            let result = if success != gl::TRUE as i32 {
                let mut info_len: c_int = 0;
                gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut info_len);
                let mut info = Vec::with_capacity(info_len as usize);
                gl::GetShaderInfoLog(shader, info_len, ptr::null_mut(), info.as_mut_ptr() as *mut gl::types::GLchar);

                // ignore null for str::from_utf8
                let info_len = match info_len {
                    0 => 0,
                    _ => (info_len - 1) as usize,
                };
                info.set_len(info_len);
                Some(String::from_utf8_unchecked(info))
            } else {
                None
            };
            gl::DeleteShader(shader);
            result
        }
    }

    pub fn vendor(&self) -> String {
        unsafe { String::from_utf8_unchecked(CStr::from_ptr(gl::GetString(gl::VENDOR) as *const _).to_bytes().to_vec()) }
    }
}

unsafe impl Sync for OpenGlContext {}

unsafe impl Send for OpenGlContext {}
