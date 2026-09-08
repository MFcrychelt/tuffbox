//! Minimal OpenGL helpers loaded via wglGetProcAddress / opengl32.

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use windows::core::PCSTR;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

pub type GLenum = u32;
pub type GLuint = u32;
pub type GLint = i32;
pub type GLsizei = i32;
pub type GLfloat = f32;
pub type GLbitfield = u32;
pub type GLboolean = u8;

pub const GL_TEXTURE_2D: GLenum = 0x0DE1;
pub const GL_RGBA: GLenum = 0x1908;
pub const GL_UNSIGNED_BYTE: GLenum = 0x1401;
pub const GL_LINEAR: GLenum = 0x2601;
pub const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub const GL_BLEND: GLenum = 0x0BE2;
pub const GL_SRC_ALPHA: GLenum = 0x0302;
pub const GL_ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub const GL_MODELVIEW: GLenum = 0x1700;
pub const GL_PROJECTION: GLenum = 0x1701;
pub const GL_COLOR_BUFFER_BIT: GLbitfield = 0x00004000;
pub const GL_QUADS: GLenum = 0x0007;
pub const GL_DEPTH_TEST: GLenum = 0x0B71;
pub const GL_CULL_FACE: GLenum = 0x0B44;

type WglGetProcAddressFn = unsafe extern "system" fn(PCSTR) -> *mut core::ffi::c_void;

static PROCS: OnceCell<GlProcs> = OnceCell::new();
static VIEWPORT: Mutex<(i32, i32, i32, i32)> = Mutex::new((0, 0, 0, 0));

pub struct GlProcs {
    pub gen_textures: unsafe extern "system" fn(GLsizei, *mut GLuint),
    pub delete_textures: unsafe extern "system" fn(GLsizei, *const GLuint),
    pub bind_texture: unsafe extern "system" fn(GLenum, GLuint),
    pub tex_image_2d: unsafe extern "system" fn(
        GLenum,
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLint,
        GLenum,
        GLenum,
        *const core::ffi::c_void,
    ),
    pub tex_sub_image_2d: unsafe extern "system" fn(
        GLenum,
        GLint,
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLenum,
        GLenum,
        *const core::ffi::c_void,
    ),
    pub tex_parameteri: unsafe extern "system" fn(GLenum, GLenum, GLint),
}

fn load_proc(name: &[u8]) -> *mut core::ffi::c_void {
    unsafe {
        let wgl: WglGetProcAddressFn = {
            let opengl = GetModuleHandleA(PCSTR::from_raw(b"opengl32.dll\0".as_ptr())).ok();
            let Some(m) = opengl else {
                return std::ptr::null_mut();
            };
            let p = GetProcAddress(m, PCSTR::from_raw(b"wglGetProcAddress\0".as_ptr()));
            match p {
                Some(f) => std::mem::transmute(f),
                None => return std::ptr::null_mut(),
            }
        };
        let mut p = wgl(PCSTR::from_raw(name.as_ptr()));
        if p.is_null() {
            if let Ok(m) = GetModuleHandleA(PCSTR::from_raw(b"opengl32.dll\0".as_ptr())) {
                if let Some(f) = GetProcAddress(m, PCSTR::from_raw(name.as_ptr())) {
                    p = f as *mut _;
                }
            }
        }
        p
    }
}

macro_rules! load {
    ($name:expr) => {{
        let p = load_proc($name);
        if p.is_null() {
            return None;
        }
        std::mem::transmute(p)
    }};
}

pub fn ensure_procs() -> Option<&'static GlProcs> {
    if let Some(p) = PROCS.get() {
        return Some(p);
    }
    unsafe {
        let procs = GlProcs {
            gen_textures: load!(b"glGenTextures\0"),
            delete_textures: load!(b"glDeleteTextures\0"),
            bind_texture: load!(b"glBindTexture\0"),
            tex_image_2d: load!(b"glTexImage2D\0"),
            tex_sub_image_2d: load!(b"glTexSubImage2D\0"),
            tex_parameteri: load!(b"glTexParameteri\0"),
        };
        let _ = PROCS.set(procs);
    }
    PROCS.get()
}

pub fn begin_overlay_frame(_hdc: HDC) {
    unsafe {
        let mut vp = [0i32; 4];
        glGetIntegerv(0x0BA2, vp.as_mut_ptr()); // GL_VIEWPORT
        *VIEWPORT.lock() = (vp[0], vp[1], vp[2], vp[3]);
        let w = if vp[2] > 0 { vp[2] } else { 1280 };
        let h = if vp[3] > 0 { vp[3] } else { 720 };

        glPushAttrib(0x000FFFFF);
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_CULL_FACE);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        glMatrixMode(GL_PROJECTION);
        glPushMatrix();
        glLoadIdentity();
        glOrtho(0.0, w as f64, h as f64, 0.0, -1.0, 1.0);
        glMatrixMode(GL_MODELVIEW);
        glPushMatrix();
        glLoadIdentity();
    }
}

pub fn end_overlay_frame() {
    unsafe {
        glMatrixMode(GL_MODELVIEW);
        glPopMatrix();
        glMatrixMode(GL_PROJECTION);
        glPopMatrix();
        glPopAttrib();
    }
}

pub fn viewport_size() -> (i32, i32) {
    let (_, _, w, h) = *VIEWPORT.lock();
    (w.max(1), h.max(1))
}

pub fn fill_rect(x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
    unsafe {
        glDisable(GL_TEXTURE_2D);
        glColor4f(r, g, b, a);
        glBegin(GL_QUADS);
        glVertex2f(x, y);
        glVertex2f(x + w, y);
        glVertex2f(x + w, y + h);
        glVertex2f(x, y + h);
        glEnd();
        glColor4f(1.0, 1.0, 1.0, 1.0);
    }
}

pub fn textured_rect(tex: GLuint, x: f32, y: f32, w: f32, h: f32) {
    if tex == 0 {
        fill_rect(x, y, w, h, 0.2, 0.2, 0.25, 1.0);
        return;
    }
    unsafe {
        glEnable(GL_TEXTURE_2D);
        glBindTexture(GL_TEXTURE_2D, tex);
        glColor4f(1.0, 1.0, 1.0, 1.0);
        glBegin(GL_QUADS);
        glTexCoord2f(0.0, 0.0);
        glVertex2f(x, y);
        glTexCoord2f(1.0, 0.0);
        glVertex2f(x + w, y);
        glTexCoord2f(1.0, 1.0);
        glVertex2f(x + w, y + h);
        glTexCoord2f(0.0, 1.0);
        glVertex2f(x, y + h);
        glEnd();
        glBindTexture(GL_TEXTURE_2D, 0);
        glDisable(GL_TEXTURE_2D);
    }
}

pub fn create_rgba_texture(width: i32, height: i32, pixels: &[u8]) -> GLuint {
    let Some(p) = ensure_procs() else {
        return 0;
    };
    let mut tex = 0u32;
    unsafe {
        (p.gen_textures)(1, &mut tex);
        (p.bind_texture)(GL_TEXTURE_2D, tex);
        (p.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        (p.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        (p.tex_image_2d)(
            GL_TEXTURE_2D,
            0,
            GL_RGBA as i32,
            width,
            height,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels.as_ptr() as *const _,
        );
        (p.bind_texture)(GL_TEXTURE_2D, 0);
    }
    tex
}

pub fn update_rgba_texture(tex: GLuint, width: i32, height: i32, pixels: &[u8]) {
    let Some(p) = ensure_procs() else {
        return;
    };
    if tex == 0 {
        return;
    }
    unsafe {
        (p.bind_texture)(GL_TEXTURE_2D, tex);
        (p.tex_sub_image_2d)(
            GL_TEXTURE_2D,
            0,
            0,
            0,
            width,
            height,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels.as_ptr() as *const _,
        );
        (p.bind_texture)(GL_TEXTURE_2D, 0);
    }
}

pub fn delete_texture(tex: GLuint) {
    if tex == 0 {
        return;
    }
    if let Some(p) = ensure_procs() {
        unsafe {
            (p.delete_textures)(1, &tex);
        }
    }
}

// Legacy fixed-pipeline entry points from opengl32 (always present on Windows).
#[link(name = "opengl32")]
extern "system" {
    fn glPushAttrib(mask: GLbitfield);
    fn glPopAttrib();
    fn glDisable(cap: GLenum);
    fn glEnable(cap: GLenum);
    fn glBlendFunc(sfactor: GLenum, dfactor: GLenum);
    fn glMatrixMode(mode: GLenum);
    fn glPushMatrix();
    fn glPopMatrix();
    fn glLoadIdentity();
    fn glOrtho(left: f64, right: f64, bottom: f64, top: f64, znear: f64, zfar: f64);
    fn glColor4f(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat);
    fn glBegin(mode: GLenum);
    fn glEnd();
    fn glVertex2f(x: GLfloat, y: GLfloat);
    fn glTexCoord2f(s: GLfloat, t: GLfloat);
    fn glGetIntegerv(pname: GLenum, params: *mut GLint);
    fn glBindTexture(target: GLenum, texture: GLuint);
}
