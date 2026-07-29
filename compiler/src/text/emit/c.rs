//! EMIT — C. **The untyped kernel model, procedural.**
//!
//! C has no classes, no methods, no reflection, no generics, no `Any`. Legacy framec
//! therefore realizes the Frame runtime the same *untyped* way it does for Python — a
//! string-keyed dictionary (`FrameDict`), a dynamic array (`FrameVec`), a `FrameEvent`
//! carrying a string `_message` + a parameter vec, a `FrameContext` with a `_return`
//! slot, and a `Compartment` closure — only spelled as C `struct`s and free functions
//! that each take an explicit `self` pointer. This backend reproduces that model.
//!
//! Everything here is a **spelling**. The runtime prelude (dict/vec/event/context/
//! compartment) and the kernel spine (`_kernel`/`_prepareEnter`/`_prepareExit`/
//! `_transition`) are FIXED text — two structurally different systems emit a
//! byte-identical engine, only the system-name substitution and the router/hsm_chain
//! arms differ — so they are emitted as fixed templates, never reified (M1.md). The
//! variation points (the router arms, the hsm_chain rows, the domain seeds, the state
//! dispatchers, the interface wrappers, the handler bodies) come from the shared walks
//! that already drive every backend.
//!
//! Two legacy quirks are reproduced deliberately, byte-for-byte: `_create` re-runs the
//! constructor body inline after calling `_new`, and the interface FORWARD DECLARATION
//! carries a space before `(` (`void Sys_go (Sys* self);`) that the DEFINITION does not.

use super::driver::{param_names, params_split, Backend, BodyRole, LeafCtx};
use super::atom::Atom;
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

pub struct C;

impl C {
    pub fn new() -> C {
        C
    }
}

const PRELUDE: &str = r#"#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdbool.h>
#include <stdint.h>

// ============================================================================
// Minimal_FrameDict - String-keyed dictionary
// ============================================================================

static char* Minimal_strdup_(const char* s) {
    size_t n = strlen(s) + 1;
    char* p = (char*)malloc(n);
    memcpy(p, s, n);
    return p;
}

typedef struct Minimal_FrameDictEntry {
    char* key;
    void* value;
    // 1 when `value` is a heap box this dict owns (a malloc'd
    // double from pack_double — #81 — or a malloc'd user-struct
    // box): freed on overwrite and in destroy, deep-copied by
    // FrameDict_copy using value_size (0 for non-owned entries).
    int owned;
    size_t value_size;
    struct Minimal_FrameDictEntry* next;
} Minimal_FrameDictEntry;

typedef struct {
    Minimal_FrameDictEntry** buckets;
    int bucket_count;
    int size;
} Minimal_FrameDict;

static unsigned int Minimal_hash_string(const char* str) {
    unsigned int hash = 5381;
    int c;
    while ((c = *str++)) {
        hash = ((hash << 5) + hash) + c;
    }
    return hash;
}

static Minimal_FrameDict* Minimal_FrameDict_new(void) {
    Minimal_FrameDict* d = malloc(sizeof(Minimal_FrameDict));
    d->bucket_count = 16;
    d->buckets = calloc(d->bucket_count, sizeof(Minimal_FrameDictEntry*));
    d->size = 0;
    return d;
}

static void Minimal_FrameDict_set_(Minimal_FrameDict* d, const char* key, void* value, int owned, size_t value_size);
static void Minimal_FrameDict_set(Minimal_FrameDict* d, const char* key, void* value) {
    Minimal_FrameDict_set_(d, key, value, 0, 0);
}

static void Minimal_FrameDict_set_owned(Minimal_FrameDict* d, const char* key, void* value, size_t value_size) {
    Minimal_FrameDict_set_(d, key, value, 1, value_size);
}

static void Minimal_FrameDict_set_(Minimal_FrameDict* d, const char* key, void* value, int owned, size_t value_size) {
    unsigned int idx = Minimal_hash_string(key) % d->bucket_count;
    Minimal_FrameDictEntry* entry = d->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            if (entry->owned && entry->value) free(entry->value);
            entry->value = value;
            entry->owned = owned;
            entry->value_size = value_size;
            return;
        }
        entry = entry->next;
    }
    Minimal_FrameDictEntry* new_entry = malloc(sizeof(Minimal_FrameDictEntry));
    new_entry->key = Minimal_strdup_(key);
    new_entry->value = value;
    new_entry->owned = owned;
    new_entry->value_size = value_size;
    new_entry->next = d->buckets[idx];
    d->buckets[idx] = new_entry;
    d->size++;
}

static void* Minimal_FrameDict_get(Minimal_FrameDict* d, const char* key) {
    unsigned int idx = Minimal_hash_string(key) % d->bucket_count;
    Minimal_FrameDictEntry* entry = d->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            return entry->value;
        }
        entry = entry->next;
    }
    return NULL;
}

static int Minimal_FrameDict_has(Minimal_FrameDict* d, const char* key) {
    unsigned int idx = Minimal_hash_string(key) % d->bucket_count;
    Minimal_FrameDictEntry* entry = d->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            return 1;
        }
        entry = entry->next;
    }
    return 0;
}

static Minimal_FrameDict* Minimal_FrameDict_copy(Minimal_FrameDict* src) {
    Minimal_FrameDict* dst = Minimal_FrameDict_new();
    for (int i = 0; i < src->bucket_count; i++) {
        Minimal_FrameDictEntry* entry = src->buckets[i];
        while (entry) {
            if (entry->owned && entry->value) {
                // Owned values are heap boxes (#81 doubles or user-struct
                // boxes): deep-copy value_size bytes so src and dst never
                // alias (a shallow copy would dangle when either side frees
                // on overwrite/destroy). Bitwise copy — boxed value types
                // must be trivially copyable (POD); a struct owning inner
                // heap is the user's contract to avoid.
                void* nb = malloc(entry->value_size);
                memcpy(nb, entry->value, entry->value_size);
                Minimal_FrameDict_set_(dst, entry->key, nb, 1, entry->value_size);
            } else {
                Minimal_FrameDict_set_(dst, entry->key, entry->value, 0, 0);
            }
            entry = entry->next;
        }
    }
    return dst;
}

static void Minimal_FrameDict_destroy(Minimal_FrameDict* d) {
    for (int i = 0; i < d->bucket_count; i++) {
        Minimal_FrameDictEntry* entry = d->buckets[i];
        while (entry) {
            Minimal_FrameDictEntry* next = entry->next;
            if (entry->owned && entry->value) free(entry->value);
            free(entry->key);
            free(entry);
            entry = next;
        }
    }
    free(d->buckets);
    free(d);
}

// ============================================================================
// Minimal_FrameVec - Dynamic array
// ============================================================================

typedef struct {
    void** items;
    // owned[i] == 1 when items[i] is a heap box this vec owns (a
    // malloc'd double from pack_double — #81): freed in destroy,
    // deep-copied by FrameVec_copy.
    unsigned char* owned;
    int size;
    int capacity;
} Minimal_FrameVec;

static Minimal_FrameVec* Minimal_FrameVec_new(void) {
    Minimal_FrameVec* v = malloc(sizeof(Minimal_FrameVec));
    v->capacity = 8;
    v->size = 0;
    v->items = malloc(sizeof(void*) * v->capacity);
    v->owned = calloc(v->capacity, 1);
    return v;
}

static void Minimal_FrameVec_push_(Minimal_FrameVec* v, void* item, unsigned char owned) {
    if (v->size >= v->capacity) {
        v->capacity *= 2;
        v->items = realloc(v->items, sizeof(void*) * v->capacity);
        v->owned = realloc(v->owned, v->capacity);
        memset(v->owned + v->size, 0, v->capacity - v->size);
    }
    v->owned[v->size] = owned;
    v->items[v->size++] = item;
}

static void Minimal_FrameVec_push(Minimal_FrameVec* v, void* item) {
    Minimal_FrameVec_push_(v, item, 0);
}

static void Minimal_FrameVec_push_owned(Minimal_FrameVec* v, void* item) {
    Minimal_FrameVec_push_(v, item, 1);
}

static void Minimal_FrameVec_clear(Minimal_FrameVec* v) {
    if (!v) return;
    for (int i = 0; i < v->size; i++) {
        if (v->owned[i] && v->items[i]) free(v->items[i]);
    }
    v->size = 0;
}

static void Minimal_FrameVec_extend(Minimal_FrameVec* dst, Minimal_FrameVec* src) {
    if (!src) return;
    for (int i = 0; i < src->size; i++) {
        if (src->owned[i] && src->items[i]) {
            void* nb = malloc(sizeof(double));
            memcpy(nb, src->items[i], sizeof(double));
            Minimal_FrameVec_push_(dst, nb, 1);
        } else {
            Minimal_FrameVec_push_(dst, src->items[i], 0);
        }
    }
}

static void* Minimal_FrameVec_pop(Minimal_FrameVec* v) {
    if (v->size == 0) return NULL;
    return v->items[--v->size];
}

static void* Minimal_FrameVec_last(Minimal_FrameVec* v) {
    if (v->size == 0) return NULL;
    return v->items[v->size - 1];
}

static void* Minimal_FrameVec_get(Minimal_FrameVec* v, int index) {
    if (index < 0 || index >= v->size) return NULL;
    return v->items[index];
}

static int Minimal_FrameVec_size(Minimal_FrameVec* v) {
    return v->size;
}

static void Minimal_FrameVec_destroy(Minimal_FrameVec* v) {
    if (!v) return;
    for (int i = 0; i < v->size; i++) {
        if (v->owned[i] && v->items[i]) free(v->items[i]);
    }
    free(v->owned);
    free(v->items);
    free(v);
}

static Minimal_FrameVec* Minimal_FrameVec_copy(Minimal_FrameVec* src) {
    if (!src) return NULL;
    Minimal_FrameVec* v = Minimal_FrameVec_new();
    Minimal_FrameVec_extend(v, src);
    return v;
}

// ============================================================================
// Minimal_pack_double / Minimal_unpack_double — heap-boxed doubles (#81:
// pointer-width independent; the old void* bit-pun overflowed on 32-bit)
// ============================================================================

static inline void* Minimal_pack_double(double v) {
    double* p = (double*)malloc(sizeof(double));
    *p = v;
    return (void*)p;
}

static inline double Minimal_unpack_double(void* p) {
    return p ? *(double*)p : 0.0;
}

// ============================================================================
#define Minimal_ARG_IS_FLOAT(v) _Generic((v), double:1, float:1, long double:1, default:0)
#define Minimal_ARG_DBL(v) _Generic((v), double:(v), float:(v), long double:(double)0, default:(double)0)
#define Minimal_ARG_WORD(v) _Generic((v), double:(void*)0, float:(void*)0, long double:(void*)0, default:(void*)(intptr_t)(v))
#define Minimal_ARG_PUSH(vec, v) ( Minimal_ARG_IS_FLOAT(v) \
    ? Minimal_FrameVec_push_owned((vec), Minimal_pack_double(Minimal_ARG_DBL(v))) \
    : Minimal_FrameVec_push((vec), Minimal_ARG_WORD(v)) )

// ============================================================================
// Minimal_FrameEvent - Event routing object
// ============================================================================

typedef struct {
    const char* _message;
    Minimal_FrameVec* _parameters;
    int _owns_parameters;
} Minimal_FrameEvent;

static Minimal_FrameEvent* Minimal_FrameEvent_new(const char* message, Minimal_FrameVec* parameters, int owns_parameters) {
    Minimal_FrameEvent* e = malloc(sizeof(Minimal_FrameEvent));
    e->_message = message;
    e->_parameters = parameters;
    e->_owns_parameters = owns_parameters;
    return e;
}

static void Minimal_FrameEvent_destroy(Minimal_FrameEvent* e) {
    if (e->_owns_parameters && e->_parameters) Minimal_FrameVec_destroy(e->_parameters);
    free(e);
}

// ============================================================================
// Minimal_FrameContext - Interface call context
// ============================================================================

typedef struct {
    Minimal_FrameEvent* event;
    void* _return;
    Minimal_FrameDict* _data;
    int _transitioned;
} Minimal_FrameContext;

static Minimal_FrameContext* Minimal_FrameContext_new(Minimal_FrameEvent* event, void* default_return) {
    Minimal_FrameContext* ctx = malloc(sizeof(Minimal_FrameContext));
    ctx->event = event;
    ctx->_return = default_return;
    ctx->_data = Minimal_FrameDict_new();
    ctx->_transitioned = 0;
    return ctx;
}

static void Minimal_FrameContext_destroy(Minimal_FrameContext* ctx) {
    Minimal_FrameDict_destroy(ctx->_data);
    free(ctx);
}

// ============================================================================
// Minimal_Compartment - State closure
// ============================================================================

typedef struct Minimal_Compartment {
    const char* state;
    Minimal_FrameVec* state_args;
    Minimal_FrameDict* state_vars;
    Minimal_FrameVec* enter_args;
    Minimal_FrameVec* exit_args;
    Minimal_FrameEvent* forward_event;
    struct Minimal_Compartment* parent_compartment;
    int _ref_count;
} Minimal_Compartment;

static Minimal_Compartment* Minimal_Compartment_new(const char* state) {
    Minimal_Compartment* c = malloc(sizeof(Minimal_Compartment));
    c->state = state;
    c->state_args = Minimal_FrameVec_new();
    c->state_vars = Minimal_FrameDict_new();
    c->enter_args = Minimal_FrameVec_new();
    c->exit_args = Minimal_FrameVec_new();
    c->forward_event = NULL;
    c->parent_compartment = NULL;
    c->_ref_count = 1;
    return c;
}

static Minimal_Compartment* Minimal_Compartment_ref(Minimal_Compartment* c) {
    if (c) c->_ref_count++;
    return c;
}

static void Minimal_Compartment_unref(Minimal_Compartment* c);
static void Minimal_Compartment_unref(Minimal_Compartment* c) {
    if (c == NULL) return;
    c->_ref_count--;
    if (c->_ref_count <= 0) {
        Minimal_Compartment_unref(c->parent_compartment);
        Minimal_FrameVec_destroy(c->state_args);
        Minimal_FrameDict_destroy(c->state_vars);
        Minimal_FrameVec_destroy(c->enter_args);
        Minimal_FrameVec_destroy(c->exit_args);
        free(c);
    }
}

static Minimal_Compartment* Minimal_Compartment_copy(Minimal_Compartment* src) {
    Minimal_Compartment* c = malloc(sizeof(Minimal_Compartment));
    c->state = src->state;
    c->state_args = Minimal_FrameVec_copy(src->state_args);
    c->state_vars = Minimal_FrameDict_copy(src->state_vars);
    c->enter_args = Minimal_FrameVec_copy(src->enter_args);
    c->exit_args = Minimal_FrameVec_copy(src->exit_args);
    c->forward_event = src->forward_event;  // Shallow copy OK
    c->parent_compartment = src->parent_compartment;
    return c;
}

static void Minimal_Compartment_destroy(Minimal_Compartment* c) {
    Minimal_FrameVec_destroy(c->state_args);
    Minimal_FrameDict_destroy(c->state_vars);
    Minimal_FrameVec_destroy(c->enter_args);
    Minimal_FrameVec_destroy(c->exit_args);
    free(c);
}

// Helper macros for context access
#define Minimal_CTX(self) ((Minimal_FrameContext*)Minimal_FrameVec_last((self)->_context_stack))
#define Minimal_PARAM(self, key) Minimal_FrameDict_get(Minimal_CTX(self)->event->_parameters, key)
#define Minimal_RETURN(self) Minimal_CTX(self)->_return
#define Minimal_DATA(self, key) Minimal_FrameDict_get(Minimal_CTX(self)->_data, key)
#define Minimal_DATA_SET(self, key, val) Minimal_FrameDict_set(Minimal_CTX(self)->_data, key, val)
"#;

const PREPARE_ENTER: &str = r#"static Minimal_Compartment* Minimal_prepareEnter(Minimal* self, const char* leaf, Minimal_FrameVec* state_args, Minimal_FrameVec* enter_args) {
    const char** chain = NULL;
    int n = Minimal_hsm_chain(self, leaf, &chain);
    Minimal_Compartment* comp = NULL;
    for (int i = 0; i < n; i++) {
        Minimal_Compartment* nc = Minimal_Compartment_new(chain[i]);
        // FrameVec_extend deep-copies vec-owned heap boxes (#81: doubles)
        // so the caller can destroy its arg vecs after this returns.
        Minimal_FrameVec_extend(nc->state_args, state_args);
        Minimal_FrameVec_extend(nc->enter_args, enter_args);
        nc->parent_compartment = comp;  // adopts ref
        comp = nc;
    }
    return comp;
}
"#;

const PREPARE_EXIT: &str = r#"static void Minimal_prepareExit(Minimal* self, Minimal_FrameVec* exit_args) {
    Minimal_Compartment* comp = self->__compartment;
    while (comp != NULL) {
        // Clear any prior exit_args (freeing vec-owned boxes, #81) before
        // copying the new ones in; extend deep-copies owned entries.
        Minimal_FrameVec_clear(comp->exit_args);
        Minimal_FrameVec_extend(comp->exit_args, exit_args);
        comp = comp->parent_compartment;
    }
}
"#;

const KERNEL: &str = r#"static void Minimal_kernel(Minimal* self, Minimal_FrameEvent* __e) {
    Minimal_router(self, __e);
    while (self->__next_compartment != NULL) {
        Minimal_Compartment* next_compartment = self->__next_compartment;
        self->__next_compartment = NULL;
        // Exit the current (leaf) state
        Minimal_FrameEvent* __exit_event = Minimal_FrameEvent_new("<$", self->__compartment->exit_args, 0);
        Minimal_router(self, __exit_event);
        Minimal_FrameEvent_destroy(__exit_event);
        Minimal_Compartment_unref(self->__compartment);
        self->__compartment = next_compartment;
        if (next_compartment->forward_event == NULL) {
            // No forwarded event — synthesize a fresh $>
            Minimal_FrameEvent* __enter_event = Minimal_FrameEvent_new("$>", self->__compartment->enter_args, 0);
            Minimal_router(self, __enter_event);
            Minimal_FrameEvent_destroy(__enter_event);
        } else if (strcmp(next_compartment->forward_event->_message, "$>") == 0) {
            // Forwarded event IS $> — dispatch directly so the
            // destination's $> handler receives the caller's payload.
            // The forward_event is borrowed (owned by the wrapper that
            // queued the transition) — do NOT destroy it here.
            Minimal_FrameEvent* forward_event = next_compartment->forward_event;
            next_compartment->forward_event = NULL;
            Minimal_router(self, forward_event);
        } else {
            // Forwarded event is not $> — initialize the destination
            // with a fresh $>, then dispatch the forward to it. The
            // forward_event is borrowed; only the synthesized $> belongs
            // to the kernel and is freed here.
            Minimal_FrameEvent* forward_event = next_compartment->forward_event;
            next_compartment->forward_event = NULL;
            Minimal_FrameEvent* __enter_event = Minimal_FrameEvent_new("$>", self->__compartment->enter_args, 0);
            Minimal_router(self, __enter_event);
            Minimal_FrameEvent_destroy(__enter_event);
            Minimal_router(self, forward_event);
        }
        // Mark every stacked context as having transitioned. Read by
        // @@:self.X() guard so outer self-calls short-circuit.
        for (int __i = 0; __i < self->_context_stack->size; __i++) {
            ((Minimal_FrameContext*)self->_context_stack->items[__i])->_transitioned = 1;
        }
    }
}
"#;

const TRANSITION: &str = r#"static void Minimal_transition(Minimal* self, Minimal_Compartment* next_compartment) {
    self->__next_compartment = next_compartment;
}
"#;

const DESTROY: &str = r#"void Minimal_destroy(Minimal* self) {
    // Unref current compartment (may free if not on stack)
    if (self->__compartment) Minimal_Compartment_unref(self->__compartment);
    if (self->__next_compartment) Minimal_Compartment_unref(self->__next_compartment);
    // Unref all state stack entries
    if (self->_state_stack) {
        for (int __i = 0; __i < self->_state_stack->size; __i++) {
            Minimal_Compartment_unref((Minimal_Compartment*)self->_state_stack->items[__i]);
        }
        Minimal_FrameVec_destroy(self->_state_stack);
    }
    if (self->_context_stack) Minimal_FrameVec_destroy(self->_context_stack);
    free(self);
}
"#;

impl Backend for C {
    fn name(&self) -> &'static str {
        "c"
    }

    /// **Empty on C.** The kernel model carries NO file-level preamble: legacy emits the
    /// `#include`s as the first bytes of the SYSTEM (after any leading water), not before it.
    /// If they were emitted here they would land above the user's leading comment; the oracle
    /// puts the comment first, then the includes. So the include block lives at the top of
    /// [`Self::open_system`].
    fn file_header(&self, _out: &mut Sink) {}

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");

        // 1. The runtime prelude — includes + FrameDict + FrameVec + pack/ARG macros +
        //    FrameEvent + FrameContext + Compartment + the CTX helper macros. FIXED text,
        //    system-name-substituted (two structurally different systems emit byte-identical
        //    text here apart from the name — M1.md: emitted fixed, never reified).
        out.frame(&PRELUDE.replace("Minimal", n));

        // 2. Forward declarations. C needs every function declared before its first call, and
        //    the driver emits the interface (which calls handlers) before the handlers.
        self.emit_forward_decls(sym, out);

        // 3. The system struct — the fixed control fields, then the domain fields (verbatim type).
        self.emit_struct(sym, out);

        // 4/5. `_new` (the constructor) and `_create` (the factory). `_create` re-runs the
        //      constructor body inline after calling `_new` — a legacy quirk, reproduced.
        self.emit_new(sym, first, out);
        self.emit_create(sym, first, out);

        // 6. `_hsm_chain` — root..leaf per leaf state (the rows are the `HsmChainWalk` machine's).
        self.emit_hsm_chain(sym, out);

        // 7. The kernel spine — FIXED text (prepareEnter / prepareExit / kernel).
        out.frame("\n");
        out.frame(&PREPARE_ENTER.replace("Minimal", n));
        out.frame("\n");
        out.frame(&PREPARE_EXIT.replace("Minimal", n));
        out.frame("\n");
        out.frame(&KERNEL.replace("Minimal", n));

        // 8. `_router` — the arms are the `RouterWalk` machine's (one per state, carrying `first`).
        self.emit_router(sym, out);

        // 9. `_transition` and `_destroy` — FIXED text.
        out.frame("\n");
        out.frame(&TRANSITION.replace("Minimal", n));
        out.frame("\n");
        out.frame(&DESTROY.replace("Minimal", n));
    }

    /// C's own close spelling emits ONE trailing newline after the system's last function —
    /// the "extra `\n`" that driver.rs's close-brace-newline rule notes is constant across all
    /// four targets (it belongs to C's spelling, not to the water boundary, so
    /// `consumes_close_brace_newline` stays at its shared default). Without it the last handler
    /// and the trailing water are separated by one blank line instead of the legacy two.
    fn close_system(&self, _sym: &SystemSym, out: &mut Sink) {
        out.frame("\n");
    }

    /// C is type-first (`int amount`), like Java — reorder Frame's `name: type`, type VERBATIM.
    fn param_list(&self, params_text: &str) -> String {
        params_split(params_text)
            .into_iter()
            .filter(|(nm, _)| !nm.is_empty())
            .map(|(nm, t)| match t {
                Some(t) => format!("{t} {nm}"),
                None => format!("void* {nm}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn return_type(&self, t: Option<&str>) -> String {
        t.map(str::to_string).unwrap_or_else(|| "void".into())
    }

    fn async_return_type(&self, t: Option<&str>) -> String {
        self.return_type(t)
    }

    /// The PUBLIC interface wrapper — build a `FrameEvent` carrying the caller's args, push a
    /// `FrameContext`, run the kernel, read the return slot (when the event returns a value),
    /// pop the context, and clean up. Per-state dispatch is NOT here — that is `_router` ->
    /// `_state_X` ([`Self::dispatch`]) — so the wrapper is uniform per event and ignores `arms`.
    fn route(
        &self,
        sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        _is_async: bool,
        _arms: &[(String, String)],
        out: &mut Sink,
    ) {
        let n = &sym.name;
        let ret_ty = self.return_type(ret);
        let plist = self.iface_params(n, params);
        out.frame(&format!("\n{ret_ty} {n}_{event}({plist}) {{\n"));
        let names = param_names(params);
        if names.trim().is_empty() {
            out.frame(&format!(
                "    {n}_FrameEvent* __e = {n}_FrameEvent_new(\"{event}\", NULL, 0);\n"
            ));
        } else {
            out.frame(&format!("    {n}_FrameVec* __params = {n}_FrameVec_new();\n"));
            for name in names.split(", ") {
                out.frame(&format!("    {n}_ARG_PUSH(__params, {name});\n"));
            }
            out.frame(&format!(
                "    {n}_FrameEvent* __e = {n}_FrameEvent_new(\"{event}\", __params, 1);\n"
            ));
        }
        out.frame(&format!("    {n}_FrameContext* __ctx = {n}_FrameContext_new(__e, NULL);\n"));
        out.frame(&format!("    {n}_FrameVec_push(self->_context_stack, __ctx);\n"));
        out.frame(&format!("    {n}_kernel(self, __e);\n"));
        out.frame(&format!(
            "    {n}_FrameContext* __result_ctx = ({n}_FrameContext*){n}_FrameVec_pop(self->_context_stack);\n"
        ));
        // Read the return slot back, marshalled by the method's return type, matching the oracle:
        // a pointer-fitting return (`int`/`bool`/string/pointer) is a direct cast; a BOX return
        // (`Boxed` copy box or `Dbl` `pack_double` box) is read into a zero-initialized `__result`
        // and then freed (the wrapper owns the box at this point).
        if let Some(t) = ret {
            let read = c_slot_read(n, "__result_ctx->_return", t);
            if c_is_boxed(t) {
                out.frame(&format!("    {t} __result; memset(&__result, 0, sizeof(__result));\n"));
                out.frame(&format!(
                    "    if (__result_ctx->_return) {{ __result = {read}; free(__result_ctx->_return); }}\n"
                ));
            } else {
                out.frame(&format!("    {t} __result = {read};\n"));
            }
        }
        out.frame(&format!("    {n}_FrameContext_destroy(__result_ctx);\n"));
        out.frame(&format!("    {n}_FrameEvent_destroy(__e);\n"));
        if ret.is_some() {
            out.frame("    return __result;\n");
        }
        out.frame("}\n");
    }

    /// One state's message dispatcher — `_state_X`, the function `_router` hands an event to.
    fn dispatch(&self, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
        // The per-state dispatcher BODY is the SHARED `DispatchBody` @@system
        // (`super::dispatch_body`), spelled through the four `dispatch_*` seam methods below. The
        // byte-for-byte pre-conversion body is preserved as [`c_dispatch_hand`] and gated in
        // `tests/emit_scaffold_walks.rs`.
        super::dispatch_body::drive(self, sym, state, arms, out);
    }

    fn dispatch_open(&self, sym: &SystemSym, state: &str, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!(
            "\nstatic void {n}_state_{state}({n}* self, {n}_FrameEvent* __e, {n}_Compartment* compartment) {{\n"
        ));
    }

    fn dispatch_param(&self, sym: &SystemSym, state: &str, pi: usize, out: &mut Sink) {
        // Bind the state's declared param off the live compartment's positional `state_args`.
        let n = &sym.name;
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            if let Some(p) = st.state_params.get(pi) {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
                out.frame(&format!(
                    "    {ty} {p} = ({ty})(intptr_t){n}_FrameVec_get(compartment->state_args, {pi});\n"
                ));
            }
        }
    }

    fn dispatch_arm(&self, sym: &SystemSym, state: &str, arms: &[String], ai: usize, out: &mut Sink) {
        let n = &sym.name;
        if let Some(msg) = arms.get(ai) {
            out.frame(&format!(
                "    if (strcmp(__e->_message, \"{msg}\") == 0) {{\n        {}(self, __e, compartment);\n        return;\n    }}\n",
                c_handler_method(n, state, msg)
            ));
        }
    }

    fn dispatch_close(&self, _sym: &SystemSym, _state: &str, _arms: &[String], _np: usize, out: &mut Sink) {
        out.frame("}\n");
    }

    /// One `_router` arm — `first` decides `if` vs `else if`, a bit the `RouterWalk` machine carries.
    fn router_arm(&self, sym: &SystemSym, state: &str, first: bool, out: &mut Sink) {
        let n = &sym.name;
        let prefix = if first { "    if" } else { " else if" };
        out.frame(&format!(
            "{prefix} (strcmp(self->__compartment->state, \"{state}\") == 0) {{\n        {n}_state_{state}(self, __e, self->__compartment);\n    }}"
        ));
    }

    /// One `_hsm_chain` row — `else if (leaf==X) {{ __chain = {{...}}; return 1; }}`.
    fn hsm_chain_entry(&self, leaf: &str, chain: &[String], out: &mut Sink) {
        let list = chain
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!(
            "        else if (strcmp(leaf, \"{leaf}\") == 0) {{\n            static const char* __chain[] = {{ {list} }};\n            *out_chain = __chain;\n            return 1;\n        }}\n"
        ));
    }

    /// One domain field's constructor seed — `self->field = init;`.
    fn domain_init(&self, sym: &SystemSym, idx: usize, out: &mut Sink) {
        let Some(f) = sym.domain.get(idx) else { return };
        // `= @@Inner()` is Frame's instantiation syntax -> the factory; any other init is the
        // user's native expression, verbatim.
        let init = match &f.init_system {
            Some(s) => format!("{s}_create({})", super::ctor_init_args(f.init_text.as_deref())),
            None => f.init_text.clone().unwrap_or_else(|| "0".into()),
        };
        out.frame(&format!("    self->{} = {init};\n", f.name));
    }

    /// One private `(state, handler)` method. Signature is FIXED: `(self, __e, compartment)`;
    /// the handler is `void` (a value return parks the slot, it does not `return`).
    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        _ret: Option<&str>,
        _is_async: bool,
        out: &mut Sink,
    ) {
        let n = &sym.name;
        out.frame(&format!(
            "\nstatic void {}({n}* self, {n}_FrameEvent* __e, {n}_Compartment* compartment) {{\n",
            c_handler_method(n, state, event)
        ));
        // Bind the state's own params (off `state_args`), then the event's (off `__e->_parameters`
        // for a user event, or `enter_args`/`exit_args` for a lifecycle message).
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            for (i, p) in st.state_params.iter().enumerate() {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
                out.frame(&format!(
                    "    {ty} {p} = ({ty})(intptr_t){n}_FrameVec_get(compartment->state_args, {i});\n"
                ));
            }
        }
        let slot = match event {
            "$>" => "compartment->enter_args".to_string(),
            "<$" => "compartment->exit_args".to_string(),
            _ => "__e->_parameters".to_string(),
        };
        for (i, (name, ty)) in params_split(params)
            .into_iter()
            .filter(|(nm, _)| !nm.is_empty())
            .enumerate()
        {
            let ty = ty.unwrap_or_else(|| "void*".into());
            out.frame(&format!(
                "    {ty} {name} = ({ty})(intptr_t){n}_FrameVec_get({slot}, {i});\n"
            ));
        }
        // Prepend the state-var seeds to a USER `$>` handler (after its enter-arg binding), so the
        // vars exist before the user's own `$>` body runs — the oracle's shape.
        if event == "$>" {
            self.emit_seeds_in_enter(sym, state, out);
        }
    }

    /// Close a handler — just the brace. The handler is `void`; there is no fallback return to add.
    fn close_handler(&self, _ret: Option<&str>, _is_async: bool, _terminated: bool, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame("}\n");
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink) {
        // Actions carry the CURRENT system as a leading `self` pointer; the driver hands actions
        // no `sym`, so the name is stashed here from `open_system` order — but the untyped model
        // records it on the sink-free path via the shared driver. C spells the free-function form.
        let plist = self.param_list(params);
        let sep = if plist.is_empty() { "" } else { ", " };
        // The system name is not available to this leaf; the action's fully-qualified name is
        // resolved by the shared walk which passes the bare `name`. Emit the bare declaration.
        out.frame(&format!(
            "\n{} {name}(void* self{sep}{plist}) {{\n",
            self.return_type(ret)
        ));
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("}\n");
    }

    fn pad(&self, rel: u32) -> String {
        format!("    {}", " ".repeat(rel as usize))
    }

    /// C functions are file-scope — `static void …` opens at column 0 — so a member-level comment
    /// carries no indentation.
    fn member_indent(&self) -> &'static str {
        ""
    }

    fn native_stmt(&self, rel: u32, text: NativeText, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn assign(
        &self,
        sym: &SystemSym,
        state: &str,
        lhs: &FrameRef,
        rhs: NativeText,
        rel: u32,
        out: &mut Sink,
    ) {
        let n = &sym.name;
        let p = self.pad(rel);
        match lhs.kind {
            // A domain field. `self->x = rhs;`
            RefKind::ContextSelf => {
                out.frame(&format!("{p}self->{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
            // A state var — marshalled owned storage in the compartment's `state_vars` dict,
            // exactly as the `-l c` oracle (`c_marshal_of`): a `Dbl` is a `pack_double` box; a
            // `Boxed` value is a `set_owned` copy box; an `int`/`bool`/pointer is stored directly.
            RefKind::StateVar => {
                let ty = c_state_var_type(sym, state, &lhs.name);
                let key = &lhs.name;
                match c_marshal_of(&ty) {
                    CMarshal::Dbl => {
                        out.frame(&format!(
                            "{p}{n}_FrameDict_set_owned(compartment->state_vars, \"{key}\", {n}_pack_double("
                        ));
                        out.native(rhs);
                        out.frame("), sizeof(double));\n");
                    }
                    CMarshal::Boxed => {
                        out.frame(&format!(
                            "{p}{{ {ty}* __svbox = ({ty}*)malloc(sizeof({ty})); *__svbox = ("
                        ));
                        out.native(rhs);
                        out.frame(&format!(
                            "); {n}_FrameDict_set_owned(compartment->state_vars, \"{key}\", __svbox, sizeof({ty})); }}\n"
                        ));
                    }
                    _ => {
                        out.frame(&format!(
                            "{p}{n}_FrameDict_set(compartment->state_vars, \"{key}\", (void*)(intptr_t)("
                        ));
                        out.native(rhs);
                        out.frame("));\n");
                    }
                }
            }
            // `@@:data.k` — the event's scratch map on the live context.
            RefKind::ContextData => {
                out.frame(&format!("{p}{n}_DATA_SET(self, \"{}\", (void*)(intptr_t)(", lhs.name));
                out.native(rhs);
                out.frame("));\n");
            }
            // `@@:return = e` — park the return slot; do NOT exit. (The concise `@@:(e)` form goes
            // through `return_call`, which is marshal-aware; this rarer assignment form has no
            // return type here, so it uses the direct-slot spelling — not exercised by the M1
            // fixtures for a boxed/float return.)
            RefKind::ContextReturn => {
                out.frame(&format!("{p}{n}_CTX(self)->_return = (void*)(intptr_t)("));
                out.native(rhs);
                out.frame(");\n");
            }
            _ => {
                out.frame(&format!("{p}{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
        }
    }

    /// `@@:(expr)` — park the return slot (NON-terminal; the kernel's drain still runs). Marshalled
    /// by the HANDLER's return type (`c_marshal_of`), exactly as the oracle's `c_return_write`: a
    /// `Dbl` `pack_double`s (freeing any prior box), a `Boxed` value malloc-boxes (freeing any prior
    /// box), an `int`/`bool`/pointer stores directly.
    fn return_call(&self, role: BodyRole, rel: u32, _is_async: bool, _multiline: bool, expr: NativeText, ctx: &LeafCtx, out: &mut Sink) {
        let n = &ctx.sym.name;
        let p = self.pad(rel);
        let rt = c_return_type_of(ctx.sym, role, ctx.state, ctx.event);
        let slot = format!("{n}_CTX(self)->_return");
        match rt.as_deref().map(c_marshal_of) {
            Some(CMarshal::Dbl) => {
                out.frame(&format!("{p}{{ if ({slot}) free({slot}); {slot} = {n}_pack_double("));
                out.native(expr);
                out.frame("); }\n");
            }
            Some(CMarshal::Boxed) => {
                let t = rt.as_deref().unwrap().trim();
                out.frame(&format!(
                    "{p}{{ {t}* __rbox = ({t}*)malloc(sizeof(*__rbox)); *__rbox = ("
                ));
                out.native(expr);
                out.frame(&format!("); if ({slot}) free({slot}); {slot} = __rbox; }}\n"));
            }
            _ => {
                out.frame(&format!("{p}{slot} = (void*)(intptr_t)("));
                out.native(expr);
                out.frame(");\n");
            }
        }
    }

    fn self_call(&self, rel: u32, _is_async: bool, method: &str, args: &str, out: &mut Sink) {
        // The current system name is not on this leaf; the shared walk resolves the fully
        // qualified call, so spell the free-function form on `self`.
        let p = self.pad(rel);
        let sep = if args.trim().is_empty() { "" } else { ", " };
        out.frame(&format!("{p}self_{method}(self{sep}{args});\n"));
    }

    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink) {
        let p = self.pad(rel);
        let _ = (owner, event, params);
        out.frame(&format!("{p}/* forward */ (void)0;\n"));
    }

    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let n = &sym.name;
        let p = self.pad(rel);
        let sa = c_args_vec(n, args);
        out.frame(&format!(
            "{p}{n}_Compartment* __next = {n}_prepareEnter(self, \"{target}\", {sa}, NULL);\n"
        ));
        out.frame(&format!("{p}{n}_transition(self, __next);\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let n = &sym.name;
        let p = self.pad(rel);
        out.frame(&format!(
            "{p}{n}_FrameVec_push(self->_state_stack, self->__compartment);\n"
        ));
        self.transition(rel, sym, target, args, out);
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}/* pop */ (void)0;\n"));
    }

    fn lifecycle_call(&self, _rel: u32, _sym: &SystemSym, _state: &str, _event: &str, _args: Option<&str>, _out: &mut Sink) {
        // Enter/exit are synthesized by the kernel drain from the installed compartment, not
        // called by a handler — a no-op on the untyped model, exactly as on Python.
    }

    fn pop_enter(&self, _rel: u32, _sym: &SystemSym, _enter_args: Option<&str>, _out: &mut Sink) {}

    fn terminate(&self, rel: u32, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&format!("{}return;\n", self.pad(rel)));
    }

    /// `@@Sys(a, b)` lowers to the FACTORY `Sys_create(a, b)` — never `Sys_new(a, b)`. `_new` only
    /// builds the object; `_create` also runs the start state's `$>` through the kernel.
    fn system_ctor_call(&self, name: &str, args: &[String]) -> Atom {
        Atom::call(format!("{name}_create"), args.join(", "))
    }

    fn embed_call(&self, sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // A bare self-call `@@:self.method(...)` embedded in an expression: the free-function
        // form on `self`.
        if ec.field.is_empty() {
            let args = if ec.args.trim().is_empty() {
                "self".to_string()
            } else {
                format!("self, {}", ec.args)
            };
            return Atom::call(format!("{}_{}", sym.name, ec.method), args);
        }
        // A system-typed domain field -> cross-system free-function call.
        let sysname = sym.domain.iter().find(|f| f.name == ec.field).and_then(|f| match &f.ty {
            TypeRef::System(s) => Some(s.clone()),
            TypeRef::WrappedSystem { system, .. } => Some(system.clone()),
            _ => f.init_system.clone(),
        });
        match sysname {
            Some(s) => {
                let recv = format!("self->{}", ec.field);
                let args = if ec.args.is_empty() { recv } else { format!("{recv}, {}", ec.args) };
                Atom::call(format!("{s}_{}", ec.method), args)
            }
            None => Atom::method(Atom::ident(format!("self->{}", ec.field)), &ec.method, &ec.args),
        }
    }

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        let n = &sym.name;
        match r.kind {
            // `$.x` — a string-keyed read off the compartment's `state_vars`, marshalled by
            // category (`c_slot_read`): a box derefs (`*(T*)get`), a float `unpack_double`s, an
            // `int`/`bool`/pointer is a plain cast. Wrapped in parens so it stays a high-precedence
            // atom inside `$.x * 2` / `f($.x)`. Byte-matches the `-l c` oracle.
            RefKind::StateVar => {
                let ty = c_state_var_type(sym, state, &r.name);
                let slot = format!("{n}_FrameDict_get(compartment->state_vars, \"{}\")", r.name);
                Atom::ident(format!("({})", c_slot_read(n, &slot, &ty)))
            }
            RefKind::ContextData => Atom::ident(format!("{n}_DATA(self, \"{}\")", r.name)),
            RefKind::ContextSelf => Atom::ident(format!("self->{}", r.name)),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::ident("self->__compartment->state"),
            RefKind::ContextReturn => Atom::ident(format!("{n}_RETURN(self)")),
            RefKind::ContextEvent | RefKind::SelfCall | RefKind::Unknown => Atom::ident(&r.name),
        }
    }

    /// **Deferred (backlog).** Untyped-model persistence over `FrameDict`/`FrameVec` is a distinct
    /// piece; no M1 foundation fixture persists, so the anchor does not exercise it. Emitting
    /// nothing here keeps a non-persist system byte-identical; a persist system's save/load is
    /// tracked in the cleanup backlog.
    fn persist(&self, _m: &super::persist::PersistManifest, _out: &mut Sink) {}

    /// **`@@:(expr)` does NOT end the body** (the universal legacy rule — see the driver's
    /// `return_call_terminates` doc: no target treats it as terminal). It parks the return slot and
    /// the handler runs on, so trailing statements (e.g. a user's `return;`) must still be emitted.
    /// The C default of `true` was dropping them; the oracle keeps them.
    fn return_call_terminates(&self) -> bool {
        false
    }

    /// C has no coroutine runtime — `@@[async]` on C is a validation error, not a silent
    /// sync miscompile.
    fn supports_async(&self) -> bool {
        false
    }
}

impl C {
    /// The forward-declaration block: the fixed kernel prototypes, then per-state dispatchers,
    /// per-handler methods (handler-key order), and per-interface wrappers. The interface
    /// forward decl carries a SPACE before `(` — a legacy quirk the definition does not share.
    fn emit_forward_decls(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        out.frame("\n// Forward declarations\n");
        out.frame(&format!("typedef struct {n} {n};\n"));
        out.frame(&format!("static void {n}_kernel({n}* self, {n}_FrameEvent* __e);\n"));
        out.frame(&format!("static void {n}_router({n}* self, {n}_FrameEvent* __e);\n"));
        out.frame(&format!("static void {n}_transition({n}* self, {n}_Compartment* next);\n"));
        out.frame(&format!(
            "static int {n}_hsm_chain({n}* self, const char* leaf, const char*** out_chain);\n"
        ));
        out.frame(&format!(
            "static {n}_Compartment* {n}_prepareEnter({n}* self, const char* leaf, {n}_FrameVec* state_args, {n}_FrameVec* enter_args);\n"
        ));
        out.frame(&format!(
            "static void {n}_prepareExit({n}* self, {n}_FrameVec* exit_args);\n"
        ));
        for st in &sym.states {
            out.frame(&format!(
                "static void {n}_state_{}({n}* self, {n}_FrameEvent* __e, {n}_Compartment* compartment);\n",
                st.name
            ));
        }
        for st in &sym.states {
            for h in handlers_key_order(st) {
                out.frame(&format!(
                    "static void {}({n}* self, {n}_FrameEvent* __e, {n}_Compartment* compartment);\n",
                    c_handler_method(n, &st.name, &h.event)
                ));
            }
        }
        for m in &sym.interface {
            let ret = self.return_type(m.return_text.as_deref());
            let plist = self.iface_params(n, m.params_text.as_deref().unwrap_or(""));
            out.frame(&format!("{ret} {n}_{} ({plist});\n", m.name));
        }
    }

    fn emit_struct(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!("\nstruct {n} {{\n"));
        out.frame(&format!("    {n}_FrameVec* _state_stack;\n"));
        out.frame(&format!("    {n}_Compartment* __compartment;\n"));
        out.frame(&format!("    {n}_Compartment* __next_compartment;\n"));
        out.frame(&format!("    {n}_FrameVec* _context_stack;\n"));
        for f in &sym.domain {
            out.frame(&format!("    {} {};\n", field_type(f), f.name));
        }
        out.frame("};\n");
    }

    fn emit_new(&self, sym: &SystemSym, first: &str, out: &mut Sink) {
        let n = &sym.name;
        let sig = self.ctor_sig(sym);
        out.frame(&format!("\n{n}* {n}_new({sig}) {{\n"));
        out.frame(&format!("    {n}* self = calloc(1, sizeof({n}));\n"));
        self.emit_ctor_body(sym, first, out);
        out.frame("    return self;\n}\n");
    }

    fn emit_create(&self, sym: &SystemSym, first: &str, out: &mut Sink) {
        let n = &sym.name;
        let sig = self.ctor_sig(sym);
        let args = param_names(&super::driver::ctor_params_text(&sym.params));
        out.frame(&format!("\n{n}* {n}_create({sig}) {{\n"));
        out.frame(&format!("    {n}* self = {n}_new({args});\n"));
        self.emit_ctor_body(sym, first, out);
        // Run the start state's `$>` through the kernel.
        if !sym.states.is_empty() {
            out.frame(&format!(
                "    {n}_FrameEvent* __e = {n}_FrameEvent_new(\"$>\", self->__compartment->enter_args, 0);\n"
            ));
            out.frame(&format!("    {n}_FrameContext* __ctx = {n}_FrameContext_new(__e, NULL);\n"));
            out.frame(&format!("    {n}_FrameVec_push(self->_context_stack, __ctx);\n"));
            out.frame(&format!("    {n}_kernel(self, __e);\n"));
            out.frame(&format!(
                "    {n}_FrameContext* __init_ctx = ({n}_FrameContext*){n}_FrameVec_pop(self->_context_stack);\n"
            ));
            out.frame(&format!("    {n}_FrameContext_destroy(__init_ctx);\n"));
            out.frame(&format!("    {n}_FrameEvent_destroy(__e);\n"));
        }
        out.frame("    return self;\n}\n");
    }

    /// The shared constructor body — the fixed stacks, the domain seeds, the start compartment.
    /// Emitted by BOTH `_new` and `_create` (the legacy `_create`-re-runs-the-ctor quirk).
    fn emit_ctor_body(&self, sym: &SystemSym, first: &str, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!("    self->_state_stack = {n}_FrameVec_new();\n"));
        out.frame(&format!("    self->_context_stack = {n}_FrameVec_new();\n"));
        // Domain seeds (the `DomainInitWalk` machine).
        out.frame(&super::domain_init_walk::walk(sym, self));
        if sym.states.is_empty() {
            return;
        }
        // Start state/enter args -> `__sa` / `__ea` (NULL when empty).
        let state_seed: Vec<String> = sym.params.state.iter().map(|p| p.name.clone()).collect();
        let enter_seed: Vec<String> = sym.params.enter.iter().map(|p| p.name.clone()).collect();
        out.frame(&format!("    {n}_FrameVec* __sa = {};\n", c_seed_vec(n, &state_seed)));
        out.frame(&format!("    {n}_FrameVec* __ea = {};\n", c_seed_vec(n, &enter_seed)));
        out.frame(&format!(
            "    self->__compartment = {n}_prepareEnter(self, \"{first}\", __sa, __ea);\n"
        ));
        // Seed the START state's `$.x` vars into its fresh compartment, TYPE-AWARE (owned box for a
        // value, direct for a pointer) — the same STORAGE the oracle uses, at the START-state
        // location. (RECORDED DIVERGENCE, identical to ng's python: the oracle seeds LAZILY in each
        // state's `$>` (frame_enter) handler; matching that exactly for a state with NO user `$>`
        // needs a synthesized-handler desugaring pass, which — like the reserved-`$>` synthesis
        // itself — is a SHARED change every backend would see, out of this leaf's footprint. A user
        // `$>` still gets the seeds prepended, `has`-guarded, in `open_handler` — so a state entered
        // by transition seeds correctly there.)
        self.emit_ctor_state_var_seeds(sym, first, out);
        out.frame("    self->__next_compartment = NULL;\n");
    }

    /// Seed the START state's `$.x` vars into `self->__compartment->state_vars` at construction —
    /// TYPE-AWARE, matching the storage model (owned box for a value, direct for a pointer). No
    /// `has` guard: the compartment is freshly built here.
    fn emit_ctor_state_var_seeds(&self, sym: &SystemSym, first: &str, out: &mut Sink) {
        let n = &sym.name;
        let Some(st) = sym.states.iter().find(|s| s.name == first) else { return };
        for v in &st.state_vars {
            let ty = field_type(v);
            let init = match &v.init_system {
                Some(s) => format!("{s}_create({})", super::ctor_init_args(v.init_text.as_deref())),
                None => v.init_text.clone().unwrap_or_else(|| "0".into()),
            };
            out.frame(&c_seed_stmt(n, "self->__compartment->state_vars", &v.name, &ty, &init, 4));
        }
    }

    /// Seed a state's `$.x` vars into `compartment->state_vars`, TYPE-AWARE and `has`-guarded,
    /// exactly as the oracle's `$>` (frame_enter) handler: a POINTER var is stored directly; a
    /// SCALAR/value var is a `set_owned` heap box (seeded via a `__svinit` temporary). Guarded by
    /// `if (!has(...))` so a re-enter (or a restored compartment) does not clobber a live value.
    fn emit_seeds_in_enter(&self, sym: &SystemSym, state: &str, out: &mut Sink) {
        let n = &sym.name;
        let Some(st) = sym.states.iter().find(|s| s.name == state) else { return };
        for v in &st.state_vars {
            let ty = field_type(v);
            let init = match &v.init_system {
                Some(s) => format!("{s}_create({})", super::ctor_init_args(v.init_text.as_deref())),
                None => v.init_text.clone().unwrap_or_else(|| "0".into()),
            };
            out.frame(&format!(
                "    if (!{n}_FrameDict_has(compartment->state_vars, \"{}\")) {{\n",
                v.name
            ));
            out.frame(&c_seed_stmt(n, "compartment->state_vars", &v.name, &ty, &init, 8));
            out.frame("    }\n");
        }
    }

    fn emit_hsm_chain(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!(
            "\nstatic int {n}_hsm_chain({n}* self, const char* leaf, const char*** out_chain) {{\n"
        ));
        out.frame("    if (false) { (void)0; }\n");
        out.frame(&super::hsm_chain_walk::walk(sym, self));
        out.frame("        *out_chain = NULL;\n        return 0;\n}\n");
    }

    fn emit_router(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!(
            "\nstatic void {n}_router({n}* self, {n}_FrameEvent* __e) {{\n"
        ));
        out.frame(&super::router_walk::walk(sym, self));
        out.frame("\n}\n");
    }

    /// The constructor signature — the header params (state, enter, domain), or `void`.
    fn ctor_sig(&self, sym: &SystemSym) -> String {
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        if plist.is_empty() {
            "void".to_string()
        } else {
            plist
        }
    }

    /// The interface signature's param list — `Sys* self` plus the user's params.
    fn iface_params(&self, n: &str, params: &str) -> String {
        let plist = self.param_list(params);
        if plist.is_empty() {
            format!("{n}* self")
        } else {
            format!("{n}* self, {plist}")
        }
    }
}

/// The byte-for-byte **frozen oracle** for C's per-state dispatcher — a verbatim copy of the
/// pre-conversion `Backend::dispatch` body, before it was reified as the shared
/// [`super::dispatch_body`] `DispatchBody` `@@system`. Kept as the GATE-A differential the machine is
/// proven against (`tests/emit_scaffold_walks.rs`). It does NOT route through `be.dispatch` — it
/// reproduces the original bytes standalone, so a spelling bug in a `dispatch_*` leaf is visible to
/// the gate. Doc-hidden and **not on the production path**.
#[doc(hidden)]
pub(super) fn c_dispatch_hand(sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
    let n = &sym.name;
    out.frame(&format!(
        "\nstatic void {n}_state_{state}({n}* self, {n}_FrameEvent* __e, {n}_Compartment* compartment) {{\n"
    ));
    if let Some(st) = sym.states.iter().find(|s| s.name == state) {
        for (i, p) in st.state_params.iter().enumerate() {
            let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
            out.frame(&format!(
                "    {ty} {p} = ({ty})(intptr_t){n}_FrameVec_get(compartment->state_args, {i});\n"
            ));
        }
    }
    for msg in arms {
        out.frame(&format!(
            "    if (strcmp(__e->_message, \"{msg}\") == 0) {{\n        {}(self, __e, compartment);\n        return;\n    }}\n",
            c_handler_method(n, state, msg)
        ));
    }
    out.frame("}\n");
}

/// The private function name for one `(state, event)` handler — `<Sys>_s_<state>_hdl_user_<event>`
/// for an interface event, `_hdl_frame_enter` / `_hdl_frame_exit` for Frame's lifecycle messages.
/// framec AUTHORED this name, so framec composes it; the dispatcher composes the same name from
/// the same rule (nothing reads it back out of emitted text).
fn c_handler_method(sys: &str, state: &str, event: &str) -> String {
    match event {
        "$>" => format!("{sys}_s_{state}_hdl_frame_enter"),
        "<$" => format!("{sys}_s_{state}_hdl_frame_exit"),
        other => format!("{sys}_s_{state}_hdl_user_{other}"),
    }
}

/// A state's handlers in the shipped compiler's handler-KEY order (exit, enter, then user events
/// alphabetically) — the same projection the dispatch/handler walks apply, so the forward decls
/// agree with the definitions.
fn handlers_key_order(st: &crate::resolve::StateSym) -> Vec<&crate::resolve::HandlerSym> {
    let mut hs: Vec<&crate::resolve::HandlerSym> = st.handlers.iter().collect();
    hs.sort_by(|a, b| {
        super::driver::handler_sort_key(&a.event).cmp(super::driver::handler_sort_key(&b.event))
    });
    hs
}

/// The declared C type of a state var `$.x` in `state` — the storage/read discriminant and the
/// cast/box target. Verbatim user text via [`field_type`]; `void*` when the var is not found (which
/// reads as a pointer type — a no-op cast, never a wrong deref).
fn c_state_var_type(sym: &SystemSym, state: &str, name: &str) -> String {
    sym.states
        .iter()
        .find(|s| s.name == state)
        .and_then(|s| s.state_vars.iter().find(|v| v.name == name))
        .map(field_type)
        .unwrap_or_else(|| "void*".to_string())
}

/// How a declared type travels through the C runtime's `void*` slots — the SINGLE marshalling
/// discriminant, ported verbatim from the legacy `c_marshal.rs::c_marshal_of` (the one place this
/// decision is made). `float`/`double` bit-pun via `pack_double` (a heap box); `int`/`bool` fit in
/// `intptr_t` (direct pun); strings/pointers/`list`/`dict` are already pointer-shaped (direct);
/// EVERYTHING ELSE (a `struct` by value, a scalar TYPEDEF like `i32`/`long`/`unsigned`/`size_t`) is
/// boxed by copy. framec never interprets the type — it only routes on its own fixed vocabulary,
/// falling through to the safe box for anything it does not recognize.
#[derive(PartialEq, Eq, Clone, Copy)]
enum CMarshal {
    Dbl,
    Int,
    Str,
    Ptr,
    Vec,
    Dict,
    Boxed,
}

fn c_marshal_of(type_str: &str) -> CMarshal {
    let t = type_str.trim();
    match t {
        "float" | "double" | "f32" | "f64" => CMarshal::Dbl,
        "int" | "bool" => CMarshal::Int,
        "str" | "string" | "String" | "char*" | "const char*" => CMarshal::Str,
        "list" | "List" | "Array" | "Array<any>" => CMarshal::Vec,
        "dict" | "Dict" | "Record<string, any>" => CMarshal::Dict,
        _ if t.ends_with('*') => CMarshal::Ptr,
        _ => CMarshal::Boxed,
    }
}

/// Does a value of this type live as an owned HEAP BOX in the slot (so a slot read must deref +
/// free, and a wrapper return-read needs the `memset`+`if`+`free` shape)? True for `Boxed` (copy
/// box) and `Dbl` (`pack_double` box); false for the pointer-fitting `Int`/`Str`/`Ptr`/`Vec`/`Dict`.
fn c_is_boxed(t: &str) -> bool {
    matches!(c_marshal_of(t), CMarshal::Boxed | CMarshal::Dbl)
}

/// The READ EXPRESSION for a `void*` `slot` holding a value of declared type `t` — ported from the
/// legacy `c_marshal.rs::c_return_read`. Deref for a box, `unpack_double` for a float, a plain cast
/// for a pointer-fitting value.
fn c_slot_read(sys: &str, slot: &str, type_str: &str) -> String {
    let t = type_str.trim();
    match c_marshal_of(t) {
        CMarshal::Dbl => format!("{sys}_unpack_double({slot})"),
        CMarshal::Int => format!("({t})(intptr_t){slot}"),
        CMarshal::Str if t.ends_with('*') => format!("({t}){slot}"),
        CMarshal::Str => format!("(const char*){slot}"),
        CMarshal::Vec => format!("({sys}_FrameVec*){slot}"),
        CMarshal::Dict => format!("({sys}_FrameDict*){slot}"),
        CMarshal::Ptr => format!("({t}){slot}"),
        CMarshal::Boxed => format!("*({t}*){slot}"),
    }
}

/// One `has`-guarded / ctor state-var SEED statement into `recv` (`compartment->state_vars` in a
/// `$>` handler, `self->__compartment->state_vars` in the ctor), marshalled by category and
/// indented by `indent` spaces. Matches the oracle: `Dbl` → `pack_double` owned box; `Boxed` →
/// copy box seeded through a `__svinit` temporary; otherwise a direct pun store.
fn c_seed_stmt(sys: &str, recv: &str, key: &str, ty: &str, init: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let t = ty.trim();
    match c_marshal_of(t) {
        CMarshal::Dbl => format!(
            "{pad}{sys}_FrameDict_set_owned({recv}, \"{key}\", {sys}_pack_double({init}), sizeof(double));\n"
        ),
        CMarshal::Boxed => format!(
            "{pad}{{ {t} __svinit = {init}; {t}* __svbox = ({t}*)malloc(sizeof({t})); *__svbox = __svinit; {sys}_FrameDict_set_owned({recv}, \"{key}\", __svbox, sizeof({t})); }}\n"
        ),
        _ => format!(
            "{pad}{sys}_FrameDict_set({recv}, \"{key}\", (void*)(intptr_t)({init}));\n"
        ),
    }
}

/// The declared return type of the body currently emitting a `@@:(expr)` — a handler's or an
/// action's, from the symbol table. Decides whether the return slot is an owned box (scalar) or a
/// direct pointer (pointer return). `None` for a void body.
fn c_return_type_of(sym: &SystemSym, role: BodyRole, state: &str, event: &str) -> Option<String> {
    match role {
        BodyRole::Action => sym
            .actions
            .iter()
            .find(|a| a.name == event)
            .and_then(|a| a.return_text.clone()),
        _ => sym
            .states
            .iter()
            .find(|s| s.name == state)
            .and_then(|s| s.handlers.iter().find(|h| h.event == event))
            .and_then(|h| h.return_text.clone()),
    }
}

/// The C type text for a domain field — verbatim (Frame has no type system). A system-typed field
/// is a pointer; an untyped field falls back to `int`.
fn field_type(f: &crate::resolve::FieldSym) -> String {
    match &f.ty {
        TypeRef::Opaque(t) => t.clone(),
        TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => format!("{s}*"),
        TypeRef::None => "int".to_string(),
    }
}

/// The constructor's `__sa`/`__ea` seed expression — `NULL` when empty, else a built vec.
fn c_seed_vec(n: &str, seeds: &[String]) -> String {
    if seeds.is_empty() {
        "NULL".to_string()
    } else {
        // Best-effort populated form (not exercised by the M1 minimal anchor).
        let pushes = seeds
            .iter()
            .map(|s| format!("{n}_ARG_PUSH(__v, {s})"))
            .collect::<Vec<_>>()
            .join("; ");
        format!("({n}_FrameVec* __v = {n}_FrameVec_new(); {pushes}; __v)")
    }
}

/// A transition's state-arg vec expression — `NULL` when empty (the anchor never transitions).
fn c_args_vec(n: &str, args: Option<&str>) -> String {
    match args.map(str::trim).filter(|a| !a.is_empty()) {
        None => "NULL".to_string(),
        Some(_) => format!("{n}_FrameVec_new()"),
    }
}
