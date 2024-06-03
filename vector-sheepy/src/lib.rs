#![no_std]
mod utils;

use js_sys::Array;
use wasm_bindgen::prelude::*;
use web_sys::WebGl2RenderingContext;

// If mouse moves > 200.0px in a frame, ignore velocity.
const MOUSE_JUMP_THRESHOLD: f64 = 200.0;

struct State {
    gl: WebGl2RenderingContext,

    pingpong_buffers: [web_sys::WebGlBuffer; 2],
    pingpong_idx: u8,

    pingpong_vaos: [web_sys::WebGlVertexArrayObject; 2],
    phys_program: web_sys::WebGlProgram,
    phys_mouse_location: Option<web_sys::WebGlUniformLocation>,
    phys_feedback: web_sys::WebGlTransformFeedback,

    draw_edge_program: web_sys::WebGlProgram,
    draw_face_program: web_sys::WebGlProgram,
    // Different vaos for different programs, x2 for each pingpong buf -w-
    edge_vaos: [web_sys::WebGlVertexArrayObject; 2],
    face_vaos: [web_sys::WebGlVertexArrayObject; 2],

    steadystate_buffer: web_sys::WebGlBuffer,

    edge_indices: web_sys::WebGlBuffer,
    face_indices: web_sys::WebGlBuffer,
    last_mouse: (f64, f64),
}
impl State {
    fn phys_read_vao(&self) -> &web_sys::WebGlVertexArrayObject {
        match self.pingpong_idx {
            0 => &self.pingpong_vaos[0],
            _ => &self.pingpong_vaos[1],
        }
    }
    fn edge_read_vao(&self) -> &web_sys::WebGlVertexArrayObject {
        match self.pingpong_idx {
            0 => &self.edge_vaos[0],
            _ => &self.edge_vaos[1],
        }
    }
    fn face_read_vao(&self) -> &web_sys::WebGlVertexArrayObject {
        match self.pingpong_idx {
            0 => &self.face_vaos[0],
            _ => &self.face_vaos[1],
        }
    }
    fn write_buf(&self) -> &web_sys::WebGlBuffer {
        match self.pingpong_idx {
            0 => &self.pingpong_buffers[1],
            _ => &self.pingpong_buffers[0],
        }
    }
    fn swap(&mut self) {
        self.pingpong_idx = (self.pingpong_idx + 1) % 2;
    }
}

static mut STATE: Option<State> = None;
mod shaders {
    pub const PHYS_BASE_POS_ATTRIBUTE_NAME: &'static str = "basePos";
    pub const PHYS_BASE_COLOR_ATTRIBUTE_NAME: &'static str = "baseColor";
    pub const PHYS_POS_ATTRIBUTE_NAME: &'static str = "inPos";
    pub const PHYS_VELOCITY_ATTRIBUTE_NAME: &'static str = "inVelocity";

    pub const PHYS_XFB_NAMES: [&'static str; 2] = ["pos", "velocity"];

    /// Shader to expand packed mesh data [x, y, r,g,b,a] into phys data [x, y, v=0, vy=0]
    pub const INIT_VERT: &'static str = r#"#version 300 es
    in mediump vec2 basePos;
    out mediump vec2 pos;
    out mediump vec2 velocity;
    float rand(vec2 co){
        return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
    }
    void main() {
        pos = basePos * 0.01;//basePos*2.0-1.0 + (vec2(rand(basePos), rand(basePos + 0.2)) - 0.5) * 0.2;
        velocity = vec2(0.0);
    }
    "#;
    /// Dummy fragment shader, as it's required in GLES even when RASTERIZER_DISCARD enabled :V
    pub const DUMMY_FRAG: &'static str = r#"#version 300 es
    void main() {}"#;
    pub const VERT_FRAG: &'static str = r#"#version 300 es
    out lowp vec4 color;
    in mediump float dist;
    void main() {
        mediump vec2 v = gl_PointCoord - 0.5;
        color = vec4(0.2, 0.3, 0.8, 1.0) * smoothstep(0.25, 0.0, dot(v, v)) * smoothstep(0.0, 0.2, dist);
    }"#;
    pub const PHYS_VERT: &'static str = r#"#version 300 es
    // mouse [x,y, vx, vy]
    uniform vec4 mouse;

    in mediump vec2 basePos;
    in mediump vec2 inPos;
    in mediump vec2 inVelocity;
    out mediump vec2 pos;
    out mediump vec2 velocity;
    out mediump float dist;
    const float DT = (1.0/60.0);
    void main() {
        vec4 lmouse = mouse/256.0 - vec4(1.0, 1.0, 0.0, 0.0);
        lmouse.xy *= -1.0;
        vec2 mouse_force_vec = inPos - lmouse.xy;
        // Inversely proportional to dist squared
        float mouse_influence = 10.0 * length(lmouse.zw) / dot(mouse_force_vec, mouse_force_vec);

        vec2 spring_force_vec = (basePos * 1.5 - 0.75) - inPos;
        // proportional to dist squared
        float spring_influence = dot(spring_force_vec, spring_force_vec) * 70.0;
        dist = length(spring_force_vec);

        vec2 accel = spring_force_vec/dist * spring_influence + mouse_force_vec * mouse_influence;

        pos = inPos + inVelocity * DT + 0.5 * accel * DT * DT;
        velocity = inVelocity + accel * DT;
        velocity *= dot(velocity, velocity) > 2.0 ? 0.1: 0.97;
        gl_Position = vec4(-inPos.x, inPos.y, 0.0, 1.0);
        gl_PointSize = smoothstep(0.0, 0.5, dist) * 8.0;
    }
    "#;
    pub const FACE_DRAW_FRAG: &'static str = r#"#version 300 es
    flat in mediump float dist;
    out lowp vec4 outColor;
    in lowp vec4 vertexColor;
    void main() {
        outColor = vertexColor * smoothstep(0.3, 0.0, dist);
    }
    "#;
    pub const FACE_DRAW_VERT: &'static str = r#"#version 300 es
    in highp vec2 basePos;
    in highp vec4 baseColor;
    in highp vec2 inPos;
    in highp vec2 inVelocity;
    flat out mediump float dist;
    out lowp vec4 vertexColor;
    void main() {
        dist = length((basePos * 1.5 - 0.75) - inPos);
        vertexColor = baseColor;
        gl_Position = vec4(-inPos.x, inPos.y, 0.0, 1.0);
    }
    "#;
    pub const EDGE_DRAW_FRAG: &'static str = r#"#version 300 es
    in mediump float dist;
    out lowp vec4 outColor;
    void main() {
        outColor = vec4(0.0, 0.5, 1.0, 1.0) * smoothstep(0.0, 0.4, dist);
    }
    "#;
    pub const EDGE_DRAW_VERT: &'static str = r#"#version 300 es
    in highp vec2 basePos;
    in highp vec2 inPos;
    in highp vec2 inVelocity;
    out mediump float dist;
    void main() {
        dist = length((basePos * 1.5 - 0.75) - inPos);
        gl_Position = vec4(-inPos.x, inPos.y, 0.0, 1.0);
    }
    "#;
}

// Import pre-processed model:

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
pub struct NormVertex {
    pub pos: [u16; 2],
    pub color: [u8; 4],
}

// const VERTICES : &'static [([u16; 2], [u8; 4])]
//                            ([x,y],    [r,g,b,a])

// const EDGES : &'static [[u16; 2]]
// const FACES : &'static [[u16; 3]]
// (it was lagging the editor lol)
include!("model.uwu");

// Expand the packed [x,y] data into [x,y,vx=0,vy=0]
fn populate_initial_buffer(
    gl: &WebGl2RenderingContext,
    num_verts: i32,
    packed_buf: &web_sys::WebGlBuffer,
    into_buf: &web_sys::WebGlBuffer,
) -> Result<(), JsError> {
    // Compile program instrumented with XFB
    let xfb = gl.create_transform_feedback().ok_or_else(|| JsError::new("Failed to create transform feedback"))?;
    let program = make_program(&gl, shaders::INIT_VERT, shaders::DUMMY_FRAG)?;
    gl.bind_transform_feedback(WebGl2RenderingContext::TRANSFORM_FEEDBACK, Some(&xfb));

    let varyings = Array::new_with_length(shaders::PHYS_XFB_NAMES.len() as u32);
    shaders::PHYS_XFB_NAMES
        .iter()
        .enumerate()
        .for_each(|(idx, &name)| varyings.set(idx as u32, name.into()));

    gl.transform_feedback_varyings(
        &program,
        &varyings,
        WebGl2RenderingContext::INTERLEAVED_ATTRIBS,
    );
    let program = link_program(&gl, program)?;

    let vao = build_vao(gl, &program, None, &packed_buf)?;

    gl.bind_buffer_base(
        WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER,
        0,
        Some(&into_buf),
    );

    //Perform transform!
    gl.use_program(Some(&program));
    gl.enable(WebGl2RenderingContext::RASTERIZER_DISCARD);
    gl.begin_transform_feedback(WebGl2RenderingContext::POINTS);

    gl.draw_arrays(WebGl2RenderingContext::POINTS, 0, num_verts);

    gl.end_transform_feedback();
    gl.disable(WebGl2RenderingContext::RASTERIZER_DISCARD);
    gl.use_program(None);

    //Unbind all the crud
    gl.bind_buffer_base(WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER, 0, None);
    gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, None);
    gl.bind_transform_feedback(WebGl2RenderingContext::TRANSFORM_FEEDBACK, None);
    gl.bind_vertex_array(None);

    // Cleanup.
    gl.delete_transform_feedback(Some(&xfb));
    gl.delete_vertex_array(Some(&vao));
    gl.delete_program(Some(&program));

    Ok(())
}

fn build_vao(
    gl: &WebGl2RenderingContext,
    program: &web_sys::WebGlProgram,
    dyn_buffer: Option<&web_sys::WebGlBuffer>,
    base_buffer: &web_sys::WebGlBuffer,
) -> Result<web_sys::WebGlVertexArrayObject, JsError> {
    let vao = gl.create_vertex_array().ok_or_else(|| JsError::new("Failed to create vertex array"))?;
    gl.bind_vertex_array(Some(&vao));
    // Bind immutable buffer attributes
    gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(base_buffer));
    if let Some(base_pos) =
        attrib_location_or_none(&gl, &program, shaders::PHYS_BASE_POS_ATTRIBUTE_NAME)
    {
        gl.vertex_attrib_pointer_with_i32(
            base_pos,
            2,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            true,
            core::mem::size_of::<NormVertex>() as i32,
            0,
        );
        gl.enable_vertex_attrib_array(base_pos);
    }
    if let Some(base_color) =
        attrib_location_or_none(&gl, &program, shaders::PHYS_BASE_COLOR_ATTRIBUTE_NAME)
    {
        gl.vertex_attrib_pointer_with_i32(
            base_color,
            4,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            true,
            core::mem::size_of::<NormVertex>() as i32,
            core::mem::size_of::<[u16; 2]>() as i32,
        );
        gl.enable_vertex_attrib_array(base_color);
    }

    if let Some(dyn_buffer) = dyn_buffer {
        // Bind dynamic physics buffer attributes
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(dyn_buffer));
        if let Some(pos) = attrib_location_or_none(&gl, &program, shaders::PHYS_POS_ATTRIBUTE_NAME) {
            gl.vertex_attrib_pointer_with_i32(
                pos,
                2,
                WebGl2RenderingContext::FLOAT,
                false,
                core::mem::size_of::<[f32; 4]>() as i32,
                0,
            );
            gl.enable_vertex_attrib_array(pos);
        }
        if let Some(vel) = attrib_location_or_none(&gl, &program, shaders::PHYS_VELOCITY_ATTRIBUTE_NAME)
        {
            gl.vertex_attrib_pointer_with_i32(
                vel,
                2,
                WebGl2RenderingContext::FLOAT,
                false,
                core::mem::size_of::<[f32; 4]>() as i32,
                core::mem::size_of::<[f32; 2]>() as i32,
            );
            gl.enable_vertex_attrib_array(vel);
        }
    }
    
    Ok(vao)
}
fn init(gl: WebGl2RenderingContext) -> Result<State, JsError> {
    // Startup configs
    gl.enable(WebGl2RenderingContext::BLEND);
    gl.blend_equation(WebGl2RenderingContext::FUNC_ADD);
    gl.blend_func(
        WebGl2RenderingContext::ONE,
        WebGl2RenderingContext::ONE_MINUS_SRC_ALPHA,
    );

    gl.disable(WebGl2RenderingContext::DEPTH_TEST);
    gl.disable(WebGl2RenderingContext::STENCIL_TEST);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);

    // Immutable mesh data
    let steadystate_buffer = {
        let buf = gl.create_buffer().ok_or_else(|| JsError::new("Failed to create mesh buffer"))?;

        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buf));
        gl.buffer_data_with_u8_array(
            WebGl2RenderingContext::ARRAY_BUFFER,
            bytemuck::cast_slice(VERTICES),
            WebGl2RenderingContext::STATIC_DRAW,
        );
        buf
    };

    // Setup pingpong buffers, both uninitialized but at the right size
    let pingpong_idx = 0;
    let pingpong_buffers = {
        // Vertices is x,y,  we need space for x,y,vx,vy, so double size!
        let buf_size = core::mem::size_of_val(VERTICES) as i32 * 2;

        let buffs = match [gl.create_buffer(), gl.create_buffer()] {
            [Some(a), Some(b)] => [a,b],
            _ => return Err(JsError::new("Failed to create pingpong buffer"))
        };
        // Set size
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffs[1]));
        // (Size, usage) initialize
        gl.buffer_data_with_i32(
            WebGl2RenderingContext::ARRAY_BUFFER,
            buf_size,
            WebGl2RenderingContext::STREAM_COPY,
        );

        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffs[0]));
        // (Size, usage) initialize
        gl.buffer_data_with_i32(
            WebGl2RenderingContext::ARRAY_BUFFER,
            buf_size,
            WebGl2RenderingContext::STREAM_COPY,
        );
        buffs
    };
    // Make program, instrumented with feedback
    let (phys_program, phys_feedback) = {
        // Compile, hook into xform feedback, then link.
        let program = make_program(&gl, shaders::PHYS_VERT, shaders::VERT_FRAG)?;
        let xfb = gl.create_transform_feedback().ok_or_else(|| JsError::new("Failed to create transform feedback"))?;
        gl.bind_transform_feedback(WebGl2RenderingContext::TRANSFORM_FEEDBACK, Some(&xfb));

        let varyings = Array::new_with_length(shaders::PHYS_XFB_NAMES.len() as u32);
        shaders::PHYS_XFB_NAMES
            .iter()
            .enumerate()
            .for_each(|(idx, &name)| varyings.set(idx as u32, name.into()));

        gl.transform_feedback_varyings(
            &program,
            &varyings,
            WebGl2RenderingContext::INTERLEAVED_ATTRIBS,
        );
        let program = link_program(&gl, program)?;
        (program, xfb)
    };
    // Setup pingpong buffer format, to match XFB
    // VAOs retain the buffer that is bound at time of creation as their source,
    // so we need one for each side of the pingpong!
    let pingpong_vaos = [
        build_vao(&gl, &phys_program, Some(&pingpong_buffers[0]), &steadystate_buffer)?,
        build_vao(&gl, &phys_program, Some(&pingpong_buffers[1]), &steadystate_buffer)?,
    ];

    let edge_indices = {
        let buf = gl.create_buffer().ok_or_else(|| JsError::new("Failed to create edge index buffer"))?;

        // Init second to indeterminte
        gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&buf));
        // (Size, usage) initialize
        gl.buffer_data_with_u8_array(
            WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(EDGES),
            WebGl2RenderingContext::STATIC_DRAW,
        );
        buf
    };
    let face_indices = {
        let buf = gl.create_buffer().ok_or_else(|| JsError::new("Failed to create face index buffer"))?;

        // Init second to indeterminte
        gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&buf));
        // (Size, usage) initialize
        gl.buffer_data_with_u8_array(
            WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(FACES),
            WebGl2RenderingContext::STATIC_DRAW,
        );
        buf
    };

    populate_initial_buffer(
        &gl,
        VERTICES.len() as i32,
        &steadystate_buffer,
        &pingpong_buffers[0],
    )?;

    let draw_edge_program = link_program(
        &gl,
        make_program(&gl, shaders::EDGE_DRAW_VERT, shaders::EDGE_DRAW_FRAG)?,
    )?;
    let draw_face_program = link_program(
        &gl,
        make_program(&gl, shaders::FACE_DRAW_VERT, shaders::FACE_DRAW_FRAG)?,
    )?;

    let edge_vaos = [
        build_vao(&gl, &draw_edge_program, Some(&pingpong_buffers[0]), &steadystate_buffer)?,
        build_vao(&gl, &draw_edge_program, Some(&pingpong_buffers[1]), &steadystate_buffer)?,
    ];

    let face_vaos = [
        build_vao(&gl, &draw_face_program, Some(&pingpong_buffers[0]), &steadystate_buffer)?,
        build_vao(&gl, &draw_face_program, Some(&pingpong_buffers[1]), &steadystate_buffer)?,
    ];

    Ok(State {
        phys_mouse_location: gl.get_uniform_location(&phys_program, "mouse"),

        gl,

        pingpong_buffers,
        pingpong_idx,
        pingpong_vaos,

        phys_program,
        phys_feedback,

        edge_indices,
        face_indices,
        steadystate_buffer,
        draw_edge_program,
        draw_face_program,
        edge_vaos,
        face_vaos,

        last_mouse: (-1000.0, -1000.0),
    })
}
fn attrib_location_or_none(
    context: &WebGl2RenderingContext,
    program: &web_sys::WebGlProgram,
    name: &str,
) -> Option<u32> {
    match context.get_attrib_location(&program, name) {
        -1 => None,
        v => Some(v as u32),
    }
}
fn make_shader(
    context: &WebGl2RenderingContext,
    ty: u32,
    src: &str,
) -> Result<web_sys::WebGlShader, JsError> {
    let shader = context
        .create_shader(ty)
        .ok_or_else(|| JsError::new("Failed to create shader"))?;
    context.shader_source(&shader, src);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        // Shader OK!
        Ok(shader)
    } else {
        // Shader failed....
        match context.get_shader_info_log(&shader) {
            Some(s) => Err(JsError::new(&s)),
            None => Err(JsError::new("Unknown shader error")),
        }
    }
}
fn make_program(
    context: &WebGl2RenderingContext,
    v_src: &str,
    f_src: &str, // Weirdly, fragment shaders are required for programs in WebGL
) -> Result<web_sys::WebGlProgram, JsError> {
    let program = context
        .create_program()
        .ok_or(JsError::new("Failed to create program"))?;

    let vertex = make_shader(context, WebGl2RenderingContext::VERTEX_SHADER, v_src)?;
    let fragment = make_shader(context, WebGl2RenderingContext::FRAGMENT_SHADER, f_src)?;
    context.attach_shader(&program, &vertex);
    context.attach_shader(&program, &fragment);
    context.delete_shader(Some(&vertex));
    context.delete_shader(Some(&fragment));

    Ok(program)
}

fn link_program(
    context: &WebGl2RenderingContext,
    program: web_sys::WebGlProgram,
) -> Result<web_sys::WebGlProgram, JsError> {
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        // Compile successful!
        Ok(program)
    } else {
        match context.get_program_info_log(&program) {
            Some(s) => Err(JsError::new(&s)),
            None => Err(JsError::new("Unknown link error!")),
        }
    }
}

/// Safety - mutable access to global state - do not call multiple times concurrently,
/// or at the same time that `init` is executing.
#[wasm_bindgen]
pub unsafe fn frame(mousepos_x: f64, mousepos_y: f64) -> Result<(), JsError> {
    // get mutable global state - held throughout the entire function
    let state =  STATE.as_mut().ok_or_else(|| JsError::new("No state to render!"))?;

    let velocity = {
        let delta = (
            mousepos_x - state.last_mouse.0,
            mousepos_y - state.last_mouse.1,
        );
        let speed_sq = delta.0 * delta.0 + delta.1 * delta.1;
        state.last_mouse = (mousepos_x, mousepos_y);
        if speed_sq > MOUSE_JUMP_THRESHOLD * MOUSE_JUMP_THRESHOLD {
            (0.0, 0.0)
        } else {
            delta
        }
    };
    let mouse_uniform = [
        mousepos_x as f32,
        mousepos_y as f32,
        velocity.0 as f32,
        velocity.1 as f32,
    ];

    let gl = &state.gl;

    gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, None);
    gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

    gl.use_program(Some(&state.draw_face_program));
    gl.bind_vertex_array(Some(&state.face_read_vao()));

    // Draw mesh
    gl.bind_buffer(
        WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
        Some(&state.face_indices),
    );
    gl.draw_elements_with_i32(
        WebGl2RenderingContext::TRIANGLES,
        FACES.len() as i32 * 3,
        WebGl2RenderingContext::UNSIGNED_SHORT,
        0,
    );

    gl.use_program(Some(&state.draw_edge_program));
    gl.bind_vertex_array(Some(&state.edge_read_vao()));
    // Draw wireframe
    gl.bind_buffer(
        WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
        Some(&state.edge_indices),
    );
    gl.draw_elements_with_i32(
        WebGl2RenderingContext::LINES,
        EDGES.len() as i32 * 2,
        WebGl2RenderingContext::UNSIGNED_SHORT,
        0,
    );

    state.swap();
    let gl = &state.gl;
    gl.bind_vertex_array(Some(&state.phys_read_vao()));
    gl.use_program(Some(&state.phys_program));

    // Do physics
    gl.bind_transform_feedback(
        WebGl2RenderingContext::TRANSFORM_FEEDBACK,
        Some(&state.phys_feedback),
    );
    gl.bind_buffer_base(
        WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER,
        0,
        Some(&state.write_buf()),
    );
    gl.uniform4fv_with_f32_array(state.phys_mouse_location.as_ref(), &mouse_uniform);
    gl.begin_transform_feedback(WebGl2RenderingContext::POINTS);
    gl.draw_arrays(WebGl2RenderingContext::POINTS, 0, VERTICES.len() as i32);
    gl.end_transform_feedback();


    match gl.get_error() {
        WebGl2RenderingContext::CONTEXT_LOST_WEBGL => {
            Err(JsError::new("Context lost!"))
        },
        WebGl2RenderingContext::OUT_OF_MEMORY => {
            Err(JsError::new("Out of memory!"))
        }
        _ => Ok(())
    }
}

/// Safety - mutable access to global state - do not call multiple times concurrently,
/// or at the same time that `frame` is executing.
#[wasm_bindgen]
pub unsafe fn setup(gl: WebGl2RenderingContext) -> Result<(), JsError> {
    if STATE.is_none() {
        STATE = Some(init(gl)?)
    }
    Ok(())
}
