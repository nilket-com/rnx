#define _GNU_SOURCE
#include "../image/engine_image.h"
#include "identity.h"
#ifdef RNX_FILE_ADAPTERS
#include "../image/adapter_provider.h"
extern int rnx_file_provider_open(struct rnx_adapter_provider *);
extern int rnx_file_provider_close(void *);
extern struct rnx_declaration *rnx_join_file_inventory(struct rnx_adapter_provider *);
#define PACKAGE_ENTRY rnx_engine_provider_file_v4
#else
#define PACKAGE_ENTRY rnx_engine_stock_file_v4
#endif

static int usage(const char *message) {
	fprintf(stderr, "rnx: %s\n", message);
	return 2;
}

int main(int argc, char **argv) {
	if (argc < 2 || strcmp(argv[1], "run"))
		return usage("expected run [--native=on|off] [--debug-engine] [--debug-source] [--budget N] FILE [ARG ...]");
	int32_t code = 1;
	struct rnx_stock_request request = {
		.version = 4, .size = sizeof(request), .flags = RNX_NATIVE,
		.fuel = 2000000, .exit_code = &code
	};
	int i = 2;
	while (i < argc) {
		const char *arg = argv[i];
		if (!strcmp(arg, "--native=off")) request.flags &= ~RNX_NATIVE;
		else if (!strcmp(arg, "--native=on")) request.flags |= RNX_NATIVE;
		else if (!strcmp(arg, "--debug-engine")) request.flags |= RNX_DEBUG;
		else if (!strcmp(arg, "--debug-source")) request.flags |= RNX_DEBUG_SOURCE;
		else if (!strcmp(arg, "--budget")) {
			if (++i == argc || argv[i][0] < '0' || argv[i][0] > '9')
				return usage("--budget needs a positive count of work units");
			char *end;
			errno = 0;
			request.fuel = strtoull(argv[i], &end, 10);
			if (*end || errno || !request.fuel || request.fuel == SIZE_MAX)
				return usage("invalid instruction budget");
		} else break;
		++i;
	}
	if (i == argc) return usage("run needs a file");
	request.path = argv[i++];
	request.argc = (size_t)(argc - i);
	request.argv = (const char *const *)(argv + i);
	if (!verified_entry((uintptr_t)&PACKAGE_ENTRY)) {
		fputs("engine image identity mismatch\n", stderr);
		return 1;
	}
#ifdef RNX_FILE_ADAPTERS
	struct rnx_adapter_provider provider = {0};
	if (rnx_file_provider_open(&provider)) return usage("file provider failed to open");
	struct rnx_declaration *joined = rnx_join_file_inventory(&provider);
	int status = joined ? rnx_engine_provider_file_v4(&request, &provider) : RNX_FAILED;
	free(joined);
	if (rnx_file_provider_close(provider.cookie)) return usage("file provider failed to close");
#else
	int status = rnx_engine_stock_file_v4(&request);
#endif
	if (status == RNX_OK) return code;
	if (status == RNX_INVALID) fputs("invalid engine file request\n", stderr);
	return 1;
}
