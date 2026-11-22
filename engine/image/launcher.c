#define _GNU_SOURCE
#include "engine_image.h"
#include "../launcher/identity.h"

static int verified(void) { return verified_entry((uintptr_t)&rnx_engine_run_file_v1); }

struct registry {
#ifdef TAGGED
	struct { unsigned tag; int offset; } slots[2];
#else
	int offset;
#endif
	unsigned calls;
};
static int64_t invoke(void *context, int64_t x) {
	struct registry *r = context;
	++r->calls;
#ifdef TAGGED
	int delta = r->slots[1].offset;
#else
	int delta = r->offset;
#endif
	return (int64_t)((uint64_t)x + (unsigned)delta);
}

int main(int argc, char **argv) {
	if (argc < 3 || strcmp(argv[1], "run")) return RNX_INVALID;
	struct registry registry = {
#ifdef TAGGED
		.slots = {{2,101},{1,3}},
#else
		.offset = 2,
#endif
	};
	uint8_t buffer[4096];
	size_t written = 0, needed = 0;
	struct rnx_request r = {.version=1, .size=sizeof(r), .flags=RNX_NATIVE, .fuel=2000000,
		.apply=invoke, .context=&registry, .buffer=buffer, .capacity=sizeof(buffer),
		.written=&written, .needed=&needed};
	for (int i=2; i<argc-1; ++i) {
		if (!strcmp(argv[i], "--native=off")) r.flags &= ~RNX_NATIVE;
		else if (!strcmp(argv[i], "--native=on")) r.flags |= RNX_NATIVE;
		else if (!strcmp(argv[i], "--debug-engine")) r.flags |= RNX_DEBUG;
		else if (!strcmp(argv[i], "--budget") && ++i < argc-1) {
			char *end;
			errno=0;
			if (argv[i][0]<'0' || argv[i][0]>'9') return RNX_INVALID;
			r.fuel=strtoull(argv[i], &end, 10);
			if (*end || errno || !r.fuel || r.fuel==UINT64_MAX) return RNX_INVALID;
		} else return RNX_INVALID;
	}
	r.path=(const uint8_t *)argv[argc-1];
	r.path_len=strlen(argv[argc-1]);
	if (!verified()) {
		fputs("engine image identity mismatch\n",stderr);
		return RNX_INVALID;
	}
	int32_t status=rnx_engine_run_file_v1(&r);
	if (written) fwrite(buffer,1,written,stderr);
	if (r.flags & RNX_DEBUG) fprintf(stderr,"rnx: callback-count=%u\n", registry.calls);
	return status;
}
