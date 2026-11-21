#ifndef RNX_ENGINE_IMAGE_H
#define RNX_ENGINE_IMAGE_H
#include <stddef.h>
#include <stdint.h>

enum { RNX_ABI_V1 = 1, RNX_NATIVE = 1, RNX_DEBUG = 2 };
enum { RNX_OK, RNX_INVALID, RNX_REFUSED, RNX_FAILED };
typedef int64_t (*rnx_apply)(void *, int64_t);
struct rnx_request {
	uint32_t version, size, flags, reserved;
	const uint8_t *path;
	size_t path_len;
	uint64_t fuel;
	rnx_apply apply;
	void *context;
	uint8_t *buffer;
	size_t capacity, *written, *needed;
};
_Static_assert(sizeof(struct rnx_request) == 88 &&
	offsetof(struct rnx_request, path) == 16 &&
	offsetof(struct rnx_request, apply) == 40 &&
	offsetof(struct rnx_request, needed) == 80, "unsupported ABI layout");
/* Pass an aligned, readable header and, for v1, at least the known prefix.
 * All spans and length outputs must be valid, live and disjoint during the
 * synchronous call. The UTF-8 path is length-delimited and excludes NUL.
 * Diagnostic bytes have no terminator. On INVALID outputs are untouched.
 * A callback runs on this thread, must return normally and cannot re-enter.
 * Only the two defined flag bits are accepted; reserved must be zero. */
int32_t rnx_engine_run_file_v1(const struct rnx_request *);

/* v2: synchronous checked-file handles. All requests have an aligned readable
 * two-word header and, for a supported version, the full readable prefix.
 * Fill unused fields with zero. All pointers refer to live, disjoint storage.
 * A success from OPEN atomically registers and seals a copy of the table.
 * Keep callback code and cookie alive until successful CLOSE; release every
 * handle on its issuing thread before exit. Callbacks must return normally,
 * and may not re-enter either engine ABI. A successful create transfers one
 * nonzero payload token, even if admission subsequently fails. Failure
 * transfers none. create must limit peak payload bytes to its supplied credit,
 * freeing temporary bytes before returning. read borrows for one call.
 */
enum rnx_operation { RNX_OPEN=1, RNX_COMPILE, RNX_PREPARE, RNX_DRIVE,
	RNX_REPORT, RNX_DISCARD_PROGRAM, RNX_RELEASE_INVOCATION, RNX_CLOSE };
struct rnx_handle { size_t slot; uint64_t generation; };
struct rnx_created { uint64_t token, kind; size_t retained, temporary; };
struct rnx_provider {
	void *cookie;
	int32_t (*create)(void *, size_t, struct rnx_created *);
	int64_t (*read)(void *, uint64_t);
	void (*release)(void *, uint64_t);
};
struct rnx_response { struct rnx_handle handle; int64_t scalar; size_t written, needed; };
struct rnx_argument { const uint8_t *data; size_t len; };
struct rnx_handles_request {
	uint32_t version, size, operation, flags;
	struct rnx_handle context, child;
	const struct rnx_provider *provider;
	const uint8_t *path;
	size_t path_len;
	const struct rnx_argument *argv;
	size_t argc, bytes, fuel;
	struct rnx_response *output;
	uint8_t *buffer;
	size_t capacity;
};
_Static_assert(sizeof(struct rnx_handles_request)==128 && sizeof(struct rnx_response)==40,
	"unsupported handle layout");
/* PREPARE accepts RNX_NATIVE and a Store byte allowance (INTPTR_MAX for
 * ordinary unbounded files). It consumes the program after validation and
 * reservation, including on engine prepare failure. DRIVE is single-use;
 * REPORT reads the terminal verdict, including after diagnostic truncation.
 * INVALID leaves output untouched. All other returns initialize the response.
 * Failed parent close leaves the context and all children usable. */
int32_t rnx_engine_handles_v2(const struct rnx_handles_request *);
#endif
