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
#endif
