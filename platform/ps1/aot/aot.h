/* The interface between the compiled script (hcb_aot.py's segments) and its runtime
 * (crates/rfvp/src/aot.rs). The segments are loaded on demand, so they only reach the
 * runtime through these symbols, resolved when they are packed (aot_link.py). */
#ifndef RFVP_AOT_H
#define RFVP_AOT_H

#include <stdint.h>

/* A script value: the tag in the high word, the payload in the low word. Nil is all zero, so
 * zeroed memory is Nil. */
typedef uint64_t V;

#define AOT_NIL 0
#define AOT_TRUE 1
#define AOT_INT 2
#define AOT_FLOAT 3 /* payload: the f32's bits */
#define AOT_CSTR 4  /* payload: the string's address in the script */
#define AOT_STR 5   /* payload: a runtime string */
#define AOT_TABLE 6 /* payload: a runtime table */

#define AOT_MK(tag, val) (((V) (uint32_t) (tag) << 32) | (uint32_t) (val))
#define AOT_TAG(v) ((uint32_t) ((v) >> 32))
#define AOT_VAL(v) ((uint32_t) (v))
#define AOT_BOOL(c) ((V) ((c) != 0) << 32)

/* A script function's activation: its arguments, locals and the values kept across calls. */
typedef struct Frame {
    struct Frame *parent;
    uint16_t seg;     /* the segment it runs in */
    uint16_t nslots;
    uint32_t state;   /* where in the segment it resumes (an index of the segment's entries) */
    V s[];
} Frame;

/* Binary operations of aot_arith and aot_cmp. */
enum {
    AOT_OP_ADD, AOT_OP_SUB, AOT_OP_MUL, AOT_OP_DIV, AOT_OP_MOD, AOT_OP_BITTEST,
    AOT_OP_AND, AOT_OP_OR, AOT_OP_EQ, AOT_OP_NE, AOT_OP_GT, AOT_OP_GE, AOT_OP_LT, AOT_OP_LE,
};

#define AOT_PACK(seg, nslots) ((uint32_t) (seg) | ((uint32_t) (nslots) << 16))
#define AOT_GOTO(seg, entry) ((uint32_t) (seg) | ((uint32_t) (entry) << 16))

/* The script's global variables. */
extern V *aot_g;
/* The current thread's return value. */
extern V aot_ret;
/* Set by a syscall that stops the current thread. */
extern int aot_yield;

/* A new frame for a call from `f` (seg | nslots << 16), all Nil, made the current one; `f`
 * resumes at `state` when it returns. */
Frame *aot_call(Frame *f, uint32_t pack, uint32_t state);
/* Run the new frame `c` (from aot_call) to its return: -1 when it returned (go on), else what
 * the segment returns to the driver: 0 when the thread stopped (yield, end), 1 when the driver
 * is to run the callee (calls nested too deep). */
int aot_invoke(Frame *c);
/* Leave `f` (aot_ret holds the result); returns 1 to run on, 0 when the thread ended. */
int aot_return(Frame *f);
/* Continue in another segment of the function (seg | state << 16); returns 1. */
int aot_goto(Frame *f, uint32_t to);
V aot_syscall(uint32_t id, const V *args);
V aot_table_get(V table, V key);
void aot_table_set(V *slot, V key, V value);
V aot_arith(uint32_t op, V a, V b);
int aot_cmp(uint32_t op, V a, V b);
V aot_neg(V a);

#endif
