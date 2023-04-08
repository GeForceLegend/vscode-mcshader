use std::collections::HashSet;

use lazy_static::lazy_static;
use regex::Regex;

use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;

lazy_static! {
    pub static ref RE_DIMENSION_FOLDER: Regex = Regex::new(r#"^world-?\d+"#).unwrap();
    pub static ref DEFAULT_SHADERS: HashSet<String> = {
        let mut set = HashSet::with_capacity(4316);
        for ext in ["fsh", "vsh", "gsh", "csh"] {
            set.insert(format!("composite.{}", ext));
            set.insert(format!("deferred.{}", ext));
            set.insert(format!("prepare.{}", ext));
            set.insert(format!("shadowcomp.{}", ext));
            for i in 1..=99 {
                let total_suffix = format!("{}.{}", i, ext);
                set.insert(format!("composite{}", total_suffix));
                set.insert(format!("deferred{}", total_suffix));
                set.insert(format!("prepare{}", total_suffix));
                set.insert(format!("shadowcomp{}", total_suffix));
            }
            set.insert(format!("composite_pre.{}", ext));
            set.insert(format!("deferred_pre.{}", ext));
            set.insert(format!("final.{}", ext));
            set.insert(format!("gbuffers_armor_glint.{}", ext));
            set.insert(format!("gbuffers_basic.{}", ext));
            set.insert(format!("gbuffers_beaconbeam.{}", ext));
            set.insert(format!("gbuffers_block.{}", ext));
            set.insert(format!("gbuffers_clouds.{}", ext));
            set.insert(format!("gbuffers_damagedblock.{}", ext));
            set.insert(format!("gbuffers_entities.{}", ext));
            set.insert(format!("gbuffers_entities_glowing.{}", ext));
            set.insert(format!("gbuffers_hand.{}", ext));
            set.insert(format!("gbuffers_hand_water.{}", ext));
            set.insert(format!("gbuffers_item.{}", ext));
            set.insert(format!("gbuffers_line.{}", ext));
            set.insert(format!("gbuffers_skybasic.{}", ext));
            set.insert(format!("gbuffers_skytextured.{}", ext));
            set.insert(format!("gbuffers_spidereyes.{}", ext));
            set.insert(format!("gbuffers_terrain.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout_mip.{}", ext));
            set.insert(format!("gbuffers_terrain_solid.{}", ext));
            set.insert(format!("gbuffers_textured.{}", ext));
            set.insert(format!("gbuffers_textured_lit.{}", ext));
            set.insert(format!("gbuffers_water.{}", ext));
            set.insert(format!("gbuffers_weather.{}", ext));
            set.insert(format!("shadow.{}", ext));
            set.insert(format!("shadow_cutout.{}", ext));
            set.insert(format!("shadow_solid.{}", ext));
        }
        let base_char_num = 'a' as u8;
        for suffix_num in 0u8..=25u8 {
            let suffix_char = (base_char_num + suffix_num) as char;
            set.insert(format!("composite_{}.csh", suffix_char));
            set.insert(format!("deferred_{}.csh", suffix_char));
            set.insert(format!("prepare_{}.csh", suffix_char));
            set.insert(format!("shadowcomp_{}.csh", suffix_char));
            for i in 1..=99 {
                let total_suffix = format!("{}_{}", i, suffix_char);
                set.insert(format!("composite{}.csh", total_suffix));
                set.insert(format!("deferred{}.csh", total_suffix));
                set.insert(format!("prepare{}.csh", total_suffix));
                set.insert(format!("shadowcomp{}.csh", total_suffix));
            }
        }
        set
    };
    pub static ref BASIC_EXTENSIONS: HashSet<String> = HashSet::from([
        String::from("vsh"),
        String::from("gsh"),
        String::from("fsh"),
        String::from("csh"),
        String::from("glsl"),
    ]);
    pub static ref RE_MACRO_CATCH: Regex = Regex::new(r#"^\s*#(include|line)"#).unwrap();
    pub static ref RE_MACRO_INCLUDE: Regex = Regex::new(r#"^\s*#include\s+"(.+)""#).unwrap();
    pub static ref RE_MACRO_VERSION: Regex = Regex::new(r#"(?m)^\s*#version.*$"#).unwrap();
    pub static ref RE_MACRO_LINE: Regex = Regex::new(r#"^\s*#line"#).unwrap();
    pub static ref OPENGL_CONTEXT: OpenGlContext = OpenGlContext::new();
    pub static ref DIAGNOSTICS_PARSER: DiagnosticsParser = DiagnosticsParser::new(&OPENGL_CONTEXT);
}

pub const OPTIFINE_MACROS: &str = "#define MC_VERSION 11900
#define MC_GL_VERSION 320
#define MC_GLSL_VERSION 150
#define MC_OS_WINDOWS
#define MC_GL_VENDOR_NVIDIA
#define MC_GL_RENDERER_GEFORCE
#define MC_NORMAL_MAP
#define MC_SPECULAR_MAP
#define MC_RENDER_QUALITY 1.0
#define MC_SHADOW_QUALITY 1.0
#define MC_HAND_DEPTH 0.125
#define MC_RENDER_STAGE_NONE 0
#define MC_RENDER_STAGE_SKY 1
#define MC_RENDER_STAGE_SUNSET 2
#define MC_RENDER_STAGE_SUN 4
#define MC_RENDER_STAGE_CUSTOM_SKY 3
#define MC_RENDER_STAGE_MOON 5
#define MC_RENDER_STAGE_STARS 6
#define MC_RENDER_STAGE_VOID 7
#define MC_RENDER_STAGE_TERRAIN_SOLID 8
#define MC_RENDER_STAGE_TERRAIN_CUTOUT_MIPPED 9
#define MC_RENDER_STAGE_TERRAIN_CUTOUT 10
#define MC_RENDER_STAGE_ENTITIES 11
#define MC_RENDER_STAGE_BLOCK_ENTITIES 12
#define MC_RENDER_STAGE_DESTROY 13
#define MC_RENDER_STAGE_OUTLINE 14
#define MC_RENDER_STAGE_DEBUG 15
#define MC_RENDER_STAGE_HAND_SOLID 16
#define MC_RENDER_STAGE_TERRAIN_TRANSLUCENT 17
#define MC_RENDER_STAGE_TRIPWIRE 18
#define MC_RENDER_STAGE_PARTICLES 19
#define MC_RENDER_STAGE_CLOUDS 20
#define MC_RENDER_STAGE_RAIN_SNOW 21
#define MC_RENDER_STAGE_WORLD_BORDER 22
#define MC_RENDER_STAGE_HAND_TRANSLUCENT 23
";
