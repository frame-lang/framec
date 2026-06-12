//! Expression-context expansion: state-var reads (`$.varName`)
//! and nested Frame sigils inside expression text.
//!
//! Two entry points, layered:
//!
//! - `expand_state_vars_in_expr(expr, lang, ctx)` — the
//!   per-target `$.varName` → compartment-state-var-read
//!   rewrite. Pure string transform; one match per backend
//!   covers per-handler / use-sv-comp / default ownership of
//!   the compartment reference. Deliberately not
//!   string-literal-aware: `f"count is {$.count}"` must
//!   interpolate.
//! - `expand_expression(expr, lang, ctx)` — the full nested-
//!   construct expansion. Calls the state-var pass first, then
//!   wraps the result in a synthetic `{ ... }` handler body
//!   and runs the scanner pipeline to recognise + expand any
//!   `@@:`-prefixed sigils via the parent's
//!   `generate_frame_expansion`. The synthetic-body trick keeps
//!   the scanner contract intact: scanning happens in the
//!   scanner, expansion happens in the expander; no ad-hoc
//!   string matching inside this module.
//!
//! `expand_expression` is `pub(crate)` because the operation-
//! body and handler-body emitters in the parent reach into it
//! directly; `expand_state_vars_in_expr` stays module-private.

use super::super::codegen_utils::{
    cpp_map_type, csharp_map_type, go_map_type, java_map_type, kotlin_map_type, swift_map_type,
    to_snake_case, HandlerContext,
};
use crate::frame_c::compiler::native_region_scanner::Region;
use crate::frame_c::compiler::splice::Splicer;
use crate::frame_c::visitors::TargetLanguage;

/// Expand state variable references ($.varName) and context syntax (@@) in an expression string
/// Expand all Frame segments within an expression string.
///
/// This is the proper way to handle nested Frame constructs inside
/// expressions — e.g., `@@:(@@:params.a + @@:params.b)`. The expression
/// text is wrapped in a synthetic handler body `{ expr }` and run through
/// the full scanner pipeline. Each recognized Frame segment is expanded
/// via `generate_frame_expansion`, and native text passes through.
///
/// This respects the pipeline boundary: scanning happens in the scanner,
/// expansion happens in the expander. No ad-hoc string matching.
pub(super) fn expand_expression(expr: &str, lang: TargetLanguage, ctx: &HandlerContext) -> String {
    // First expand state vars ($.x) which are handled by a dedicated function
    let with_state_vars = expand_state_vars_in_expr(expr, lang, ctx);

    // If no @@ constructs remain, return early
    if !with_state_vars.contains("@@:") && !with_state_vars.contains("@@") {
        return with_state_vars;
    }

    // Wrap expression in synthetic braces for the scanner
    let synthetic_body = format!("{{{}}}", with_state_vars);
    let body_bytes = synthetic_body.as_bytes();

    // Run the scanner on the synthetic body
    let mut scanner = super::scanner_dispatch::get_native_scanner(lang);
    let scan_result = match scanner.scan(body_bytes, 0) {
        Ok(r) => r,
        Err(_) => return with_state_vars,
    };

    if std::env::var("FRAME_DEBUG_EXPR").is_ok() {
        eprintln!(
            "[expand_expression] input='{}' regions={}",
            with_state_vars,
            scan_result.regions.len()
        );
        for (i, r) in scan_result.regions.iter().enumerate() {
            match r {
                Region::NativeText { span } => eprintln!(
                    "  [{}] NativeText {:?} = '{}'",
                    i,
                    span,
                    String::from_utf8_lossy(&body_bytes[span.start..span.end])
                ),
                Region::FrameSegment { span, kind, .. } => eprintln!(
                    "  [{}] FrameSegment {:?} {:?} = '{}'",
                    i,
                    kind,
                    span,
                    String::from_utf8_lossy(&body_bytes[span.start..span.end])
                ),
            }
        }
    }

    // Expand each Frame segment
    let mut expansions = Vec::new();
    for region in &scan_result.regions {
        if let Region::FrameSegment {
            span,
            kind,
            metadata,
            ..
        } = region
        {
            let expansion =
                super::generate_frame_expansion(body_bytes, span, *kind, 0, lang, ctx, metadata);
            expansions.push(expansion);
        }
    }

    // Splice native text + expansions
    let splicer = Splicer;
    let spliced = splicer.splice(body_bytes, &scan_result.regions, &expansions);

    // Remove synthetic braces and trim
    let text = spliced.text.trim();
    text.to_string()
}

/// Expand `$.varName` state variable references within an expression string.
/// Uses compartment.state_vars for Python/TypeScript.
/// For HSM: uses __sv_comp when ctx.use_sv_comp is true (navigates to correct parent compartment)
fn expand_state_vars_in_expr(expr: &str, lang: TargetLanguage, ctx: &HandlerContext) -> String {
    // `$.varName` is expanded even inside string literals because that's
    // the contract for Frame interpolation — e.g. `f"count is {$.count}"`
    // in Python needs `$.count` → `self.__compartment.state_vars["count"]`
    // so the f-string interpolation resolves at runtime. Equivalent
    // pattern applies to JS/TS backtick ``${ }``, Kotlin `"${ }"`, Ruby
    // `"#{}"`, Swift `"\( )"`. Any false-match would require a user
    // literally writing `$.foo` inside a non-interpolating string —
    // exceedingly unusual, and not worth losing interpolation support.
    //
    // We DO track the currently-open string delimiter so the Python
    // dict-subscript key can pick the *opposite* quote. A same-quote
    // nested f-string (`f"...{d["k"]}..."`) is a SyntaxError before
    // Python 3.12 / PEP 701 — FRAMEC_BUGS #47. Python is the only target
    // affected; every other interpolating target tolerates nested
    // same-quotes inside its substitution syntax.
    let mut result = String::new();
    let bytes = expr.as_bytes();

    // None outside any string literal; Some(b'"') / Some(b'\'') while open.
    let mut string_delim: Option<u8> = None;

    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'.' {
            // Found $.varName
            i += 2; // Skip "$."
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let var_name = String::from_utf8_lossy(&bytes[start..i]).to_string();
            match lang {
                TargetLanguage::Python3 => {
                    // Pick the opposite quote from any surrounding string so an
                    // interpolated `$.var` inside an f-string doesn't nest
                    // same-quotes (SyntaxError pre-3.12; FRAMEC_BUGS #47).
                    let q = match string_delim {
                        Some(b'"') => '\'',
                        Some(b'\'') => '"',
                        _ => '"',
                    };
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[{q}{var_name}{q}]"))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[{q}{var_name}{q}]"))
                    } else {
                        result.push_str(&format!("self.__compartment.state_vars[{q}{var_name}{q}]"))
                    }
                }
                TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!("this.__compartment.state_vars[\"{}\"]", var_name))
                    }
                }
                TargetLanguage::Rust => {
                    result.push_str(&super::super::rust_system::rust_expand_state_var_read(
                        ctx, &var_name,
                    ));
                }
                TargetLanguage::C => {
                    // Category-aware unpack via c_marshal (#77), mirroring
                    // `expand_state_var`'s C arm: float/double state-vars
                    // bit-pun through the void* slot — `(int)(intptr_t)`
                    // truncates them.
                    let c_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|s| s.as_str())
                        .unwrap_or("int");
                    let comp = if ctx.per_handler {
                        "compartment"
                    } else if ctx.use_sv_comp {
                        "__sv_comp"
                    } else {
                        "self->__compartment"
                    };
                    let slot = format!(
                        "{}_FrameDict_get({}->state_vars, \"{}\")",
                        ctx.system_name, comp, var_name
                    );
                    use super::super::c_marshal::{c_marshal_of, CMarshal};
                    let read = match c_marshal_of(c_type) {
                        CMarshal::Str => format!("(const char*){}", slot),
                        CMarshal::Dbl => {
                            format!("{}_unpack_double({})", ctx.system_name, slot)
                        }
                        _ => format!("(int)(intptr_t){}", slot),
                    };
                    result.push_str(&read);
                }
                TargetLanguage::Cpp => {
                    let cpp_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| cpp_map_type(t))
                        .unwrap_or_else(|| "int".to_string());
                    if ctx.per_handler {
                        result.push_str(&format!(
                            "std::any_cast<{}>(compartment->state_vars[\"{}\"])",
                            cpp_type, var_name
                        ))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!(
                            "std::any_cast<{}>(__sv_comp->state_vars[\"{}\"])",
                            cpp_type, var_name
                        ))
                    } else {
                        result.push_str(&format!(
                            "std::any_cast<{}>(__compartment->state_vars[\"{}\"])",
                            cpp_type, var_name
                        ))
                    }
                }
                TargetLanguage::Java => {
                    let java_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| java_map_type(t))
                        .unwrap_or_else(|| "int".to_string());
                    let accessor = if ctx.per_handler {
                        format!("compartment.state_vars.get(\"{}\")", var_name)
                    } else if ctx.use_sv_comp {
                        format!("__sv_comp.state_vars.get(\"{}\")", var_name)
                    } else {
                        format!("__compartment.state_vars.get(\"{}\")", var_name)
                    };
                    if java_type == "Object" {
                        result.push_str(&accessor);
                    } else {
                        // Wrap cast in parens so method calls chain correctly:
                        // ((String) map.get("k")).equals("v") not (String) map.get("k").equals("v")
                        result.push_str(&format!("(({}) {})", java_type, accessor));
                    }
                }
                TargetLanguage::Kotlin => {
                    let kt_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| kotlin_map_type(t))
                        .unwrap_or_else(|| "Int".to_string());
                    let cast = if kt_type == "Any?" {
                        String::new()
                    } else {
                        format!(" as {}", kt_type)
                    };
                    if ctx.per_handler {
                        result
                            .push_str(&format!("compartment.state_vars[\"{}\"]{}", var_name, cast))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]{}", var_name, cast))
                    } else {
                        result.push_str(&format!(
                            "__compartment.state_vars[\"{}\"]{}",
                            var_name, cast
                        ))
                    }
                }
                TargetLanguage::Swift => {
                    let sw_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| swift_map_type(t))
                        .unwrap_or_else(|| "Int".to_string());
                    if sw_type == "Any" {
                        if ctx.per_handler {
                            result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                        } else if ctx.use_sv_comp {
                            result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                        } else {
                            result.push_str(&format!("__compartment.state_vars[\"{}\"]", var_name))
                        }
                    } else if ctx.per_handler {
                        result.push_str(&format!(
                            "(compartment.state_vars[\"{}\"] as! {})",
                            var_name, sw_type
                        ))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!(
                            "(__sv_comp.state_vars[\"{}\"] as! {})",
                            var_name, sw_type
                        ))
                    } else {
                        result.push_str(&format!(
                            "(__compartment.state_vars[\"{}\"] as! {})",
                            var_name, sw_type
                        ))
                    }
                }
                TargetLanguage::CSharp => {
                    let cs_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| csharp_map_type(t))
                        .unwrap_or_else(|| "int".to_string());
                    let cast = if cs_type == "object" {
                        String::new()
                    } else {
                        format!("({}) ", cs_type)
                    };
                    if ctx.per_handler {
                        result
                            .push_str(&format!("{}compartment.state_vars[\"{}\"]", cast, var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("{}__sv_comp.state_vars[\"{}\"]", cast, var_name))
                    } else {
                        result.push_str(&format!(
                            "{}__compartment.state_vars[\"{}\"]",
                            cast, var_name
                        ))
                    }
                }
                TargetLanguage::Go => {
                    let go_type = ctx
                        .state_var_types
                        .get(&var_name)
                        .map(|t| go_map_type(t))
                        .unwrap_or_else(|| "int".to_string());
                    if ctx.per_handler {
                        result.push_str(&format!(
                            "compartment.stateVars[\"{}\"].({})",
                            var_name, go_type
                        ))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!(
                            "__sv_comp.stateVars[\"{}\"].({})",
                            var_name, go_type
                        ))
                    } else {
                        result.push_str(&format!(
                            "s.compartment.stateVars[\"{}\"].({})",
                            var_name, go_type
                        ))
                    }
                }
                TargetLanguage::Php => {
                    if ctx.per_handler {
                        result.push_str(&format!("$compartment->state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("$__sv_comp->state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!(
                            "$this->__compartment->state_vars[\"{}\"]",
                            var_name
                        ))
                    }
                }
                TargetLanguage::Ruby => {
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!("@__compartment.state_vars[\"{}\"]", var_name))
                    }
                }
                TargetLanguage::Lua => {
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!("self.__compartment.state_vars[\"{}\"]", var_name))
                    }
                }
                TargetLanguage::Dart => {
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!("this.__compartment.state_vars[\"{}\"]", var_name))
                    }
                }
                TargetLanguage::GDScript => {
                    if ctx.per_handler {
                        result.push_str(&format!("compartment.state_vars[\"{}\"]", var_name))
                    } else if ctx.use_sv_comp {
                        result.push_str(&format!("__sv_comp.state_vars[\"{}\"]", var_name))
                    } else {
                        result.push_str(&format!("self.__compartment.state_vars[\"{}\"]", var_name))
                    }
                }
                TargetLanguage::Erlang => {
                    // Leave `self.` placeholder so the body processor's
                    // classifier substitutes with the live `DataN#data.`
                    // — matches the live data_gen. See sibling fix in
                    // `FrameSegmentKind::StateVarRead` above.
                    let state_prefix = to_snake_case(&ctx.state_name);
                    result.push_str(&format!("self.sv_{}_{}", state_prefix, var_name))
                }
                TargetLanguage::Graphviz => unreachable!(),
            }
        } else {
            let b = bytes[i];
            // Maintain string-delimiter state for the Python key-quote swap.
            if let Some(d) = string_delim {
                // Inside a string: copy an escape pair verbatim so an
                // escaped quote doesn't toggle the delimiter; a matching
                // unescaped delimiter closes the string.
                if b == b'\\' && i + 1 < bytes.len() {
                    result.push(b as char);
                    result.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                if b == d {
                    string_delim = None;
                }
            } else if b == b'"' || b == b'\'' {
                string_delim = Some(b);
            }
            result.push(b as char);
            i += 1;
        }
    }

    result
}
