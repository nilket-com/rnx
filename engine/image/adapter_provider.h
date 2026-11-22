#ifndef RNX_ADAPTER_PROVIDER_H
#define RNX_ADAPTER_PROVIDER_H
#include "engine_image.h"

/* The provider owns payloads and uses its own allocator. All input spans,
 * including names in a registration, are borrowed for the call only.
 * kind 0 denotes no native operand. Operation IDs are local to one sealed
 * provider. All unused fields are zero. */
enum rnx_native_kind {
	RNX_NO_NATIVE, RNX_FRAME, RNX_DENSE, RNX_TENSOR, RNX_MLP, RNX_POLARS_ERROR
};
enum rnx_adapter_shape {
	RNX_CSV = 1, RNX_COLUMNS, RNX_CONVERT, RNX_LOAD, RNX_FORWARD,
	RNX_UNARY, RNX_INTEGER, RNX_PAIR, RNX_TRIPLE, RNX_TEXT,
	RNX_NAMES, RNX_DISPLAY, RNX_CSV_PRE_EFFECT_REFUSAL,
	RNX_SESSION_FACTORY, RNX_SESSION_FALLIBLE_READ, RNX_RESOLUTION_ONLY, RNX_SESSION_ERROR_TEXT
};
struct rnx_declaration {
	struct rnx_argument name;
	uint32_t id, shape, receiver, argument, result, reserved;
};
struct rnx_tree_node {
	uint32_t kind, frontier;
	size_t children;
	struct rnx_argument text;
};
enum rnx_schema_kind { RNX_OTHER, RNX_VECTOR, RNX_TUPLE, RNX_STRING };
/* A node index is meaningful only during this callback. */
struct rnx_schema {
	const void *cookie;
	int32_t (*node)(const void *, size_t, struct rnx_tree_node *);
	int32_t (*child)(const void *, size_t, size_t, size_t *);
};
struct rnx_adapter_input {
	uint64_t receiver, argument;
	size_t credit;
	struct rnx_argument path;
	struct rnx_schema schema;
	const struct rnx_argument *names;
	size_t count;
	uint32_t selection, dtype;
	struct rnx_argument detail;
};
enum rnx_output_kind {
	RNX_VALUE = 1, RNX_INNER_ERR, RNX_EXEC_ERROR, RNX_LIMIT
};
struct rnx_adapter_output {
	uint32_t outcome, reserved;
	struct rnx_created payload;
	int64_t integers[3];
	const struct rnx_argument *strings;
	size_t count;
};
/* invoke emits once, synchronously, including on failure. emit takes ownership
 * of a successfully created payload even if its subsequent validation fails.
 * The pointed-to strings remain provider-owned until emit returns. */
typedef void (*rnx_emit)(void *, const struct rnx_adapter_output *);
struct rnx_adapter_provider {
	void *cookie;
	const struct rnx_declaration *declarations;
	size_t count;
	void (*invoke)(void *, uint32_t, const struct rnx_adapter_input *, void *, rnx_emit);
	void (*release)(void *, uint64_t);
};
/* v3 shares v2's request prefix and handle sequencing. The OPEN table has a
 * distinct type and the version in every request is 3. COMPILE flag 1
 * requests the stock source id, revision and byte range for a refusal, or an
 * explicit unlocated report when the stock error carries no span. Refusal-only
 * declarations use ID zero and a fully qualified adapter path. */
struct rnx_adapter_request {
	uint32_t version, size, operation, flags;
	struct rnx_handle context, child;
	const struct rnx_adapter_provider *provider;
	const uint8_t *path;
	size_t path_len;
	const struct rnx_argument *argv;
	size_t argc, bytes, fuel;
	struct rnx_response *output;
	uint8_t *buffer;
	size_t capacity;
};
_Static_assert(sizeof(struct rnx_adapter_request) == sizeof(struct rnx_handles_request)
	&& offsetof(struct rnx_adapter_request, provider) == offsetof(struct rnx_handles_request, provider),
	"unsupported adapter request layout");
int32_t rnx_engine_adapters_v3(const struct rnx_adapter_request *);
#endif
