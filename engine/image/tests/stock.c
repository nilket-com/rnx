#include "../engine_image.h"
#include <assert.h>
#include <stdlib.h>
#include <string.h>

/* Map only a completed call to its product exit; reserve fixture-only exit
 * values for the other ABI outcomes so a confused status fails the witness. */
int main(int argc, char **argv) {
	assert(argc >= 2);
	int32_t code = -1;
	struct rnx_stock_request r = {
		.version = 4, .size = sizeof(r), .flags = RNX_NATIVE | RNX_DEBUG,
		.path = argv[1], .argv = (const char *const *)(argv + 2),
		.argc = (size_t)(argc - 2), .fuel = 2000000000, .exit_code = &code
	};
	struct rnx_stock_request bad = r;
	bad.size--;
	assert(rnx_engine_stock_file_v4(&bad) == RNX_INVALID && code == -1);
	int status = rnx_engine_stock_file_v4(&r);
	if (getenv("RNX_TEST_MUST_EXIT")) return 97;
	if (status == RNX_OK) return code;
	assert(code == -1);
	return status == RNX_REFUSED ? 78 : 79;
}
