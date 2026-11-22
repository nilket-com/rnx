/* Standalone file-adapter fixture. The ordinary run subcommand measures
 * the image through the same launcher and published scalar entry. */
#define main ordinary_file_main
#include "launcher.c"
#undef main
#include "adapter_provider.h"
extern int rnx_file_provider_open(struct rnx_adapter_provider *);
extern int rnx_file_provider_close(void *);
extern struct rnx_declaration *rnx_join_file_inventory(struct rnx_adapter_provider *);
extern int rnx_dump_file_inventory(const struct rnx_adapter_provider *);

/* A declined compile must never enter the provider, create a payload or debit credit. */
static void unexpected_invoke(void *p, uint32_t op, const struct rnx_adapter_input *input,
		void *sink, rnx_emit emit) {
	(void)p; (void)op; (void)input; (void)sink; (void)emit;
	fputs("unexpected provider invocation during a refusal control\n", stderr);
	abort();
}

int main(int argc, char **argv) {
	int refusal = argc > 1 && !strcmp(argv[1], "checked-refusal");
	int inventory = argc > 1 && (!strcmp(argv[1],"inventory") || !strcmp(argv[1],"inventory-missing"));
	if (argc < 2 || (!inventory && !refusal && strcmp(argv[1], "checked"))) return ordinary_file_main(argc, argv);
	if ((!inventory && argc < 3) || !verified_entry((uintptr_t)&rnx_engine_adapters_v3)) return RNX_INVALID;
	struct rnx_adapter_provider provider = {0};
	if (rnx_file_provider_open(&provider)) return RNX_INVALID;
	struct rnx_declaration *joined=rnx_join_file_inventory(&provider);
	if (!joined) { rnx_file_provider_close(provider.cookie); return RNX_FAILED; }
	if (inventory) {
		if (!strcmp(argv[1],"inventory-missing")) --provider.count;
		int code=rnx_dump_file_inventory(&provider);
		free(joined);
		return rnx_file_provider_close(provider.cookie) || code;
	}
	if (refusal) provider.invoke=unexpected_invoke;
	struct rnx_response out = {0};
	unsigned char *buffer = malloc(262145);
	struct rnx_argument *args = calloc((size_t)argc, sizeof(*args));
	if (!buffer || !args) {
		free(buffer);
		free(args);
		free(joined);
		rnx_file_provider_close(provider.cookie);
		return RNX_FAILED;
	}
	struct rnx_adapter_request r = {.version=3, .size=sizeof(r), .operation=RNX_OPEN,
		.provider=&provider, .output=&out, .buffer=buffer, .capacity=262144};
	struct rnx_handle context={0},program={0},invocation={0};
	int status = rnx_engine_adapters_v3(&r);
	if (status) goto done;
	context=out.handle;
	r.context=context;
	r.operation=RNX_COMPILE;
	r.flags=(uint32_t)refusal;
	r.provider=NULL;
	r.path=(const uint8_t *)argv[2];
	r.path_len=strlen(argv[2]);
	status=rnx_engine_adapters_v3(&r);
	if (status) goto done;
	program=out.handle;
	r.path=NULL;
	r.path_len=0;
	r.operation=RNX_PREPARE;
	r.child=program;
	r.argc=(size_t)(argc-3);
	for (size_t i=0; i<r.argc; ++i) {
		args[i].data=(const uint8_t *)argv[i+3];
		args[i].len=strlen(argv[i+3]);
	}
	r.argv=args;
	r.bytes=INTPTR_MAX;
	r.flags=1;
	status=rnx_engine_adapters_v3(&r);
	if (status!=RNX_INVALID) program=(struct rnx_handle){0};
	if (status) goto done;
	invocation=out.handle;
	r.argv=NULL;
	r.argc=0;
	r.bytes=0;
	r.flags=0;
	r.operation=RNX_DRIVE;
	r.child=invocation;
	r.fuel=2000000;
	status=rnx_engine_adapters_v3(&r);
done:
	if (status) {
		if (status==RNX_REFUSED) fputs("refused: ",stderr);
		fwrite(buffer,1,out.written,stderr);
		if (!out.written || buffer[out.written-1]!='\n') fputc('\n',stderr);
	}
	r=(struct rnx_adapter_request){.version=3,.size=sizeof(r),.context=context,.output=&out};
	if (invocation.generation || program.generation) {
		r.operation=invocation.generation ? RNX_RELEASE_INVOCATION : RNX_DISCARD_PROGRAM;
		r.child=invocation.generation ? invocation : program;
		if (rnx_engine_adapters_v3(&r)) status=RNX_INVALID;
	}
	if (context.generation) {
		r.operation=RNX_CLOSE;
		r.child=(struct rnx_handle){0};
		if (rnx_engine_adapters_v3(&r)) status=RNX_INVALID;
	}
	if (rnx_file_provider_close(provider.cookie)) status=RNX_INVALID;
	free(joined);
	free(buffer);
	free(args);
	return status;
}
