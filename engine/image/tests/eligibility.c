#define _GNU_SOURCE
#include "../engine_image.h"
#include <assert.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>

/* Observe the actual lookup without changing the value seen by the image. */
static _Thread_local unsigned reads;
static _Thread_local int present;
char *getenv(const char *key) {
	char *(*next)(const char *) = dlvsym(RTLD_NEXT,"getenv","GLIBC_2.2.5");
	assert(next);
	char *value=next(key);
	if (!strcmp(key,"PERF_BUILDID_DIR")) {
		++reads;
		present=value!=NULL;
	}
	return value;
}

static const char *script;
static int expected;
static int32_t unused_create(void *p,size_t n,struct rnx_created *v) {
	(void)p; (void)n; (void)v; abort();
}
static int64_t unused_read(void *p,uint64_t v) { (void)p; (void)v; abort(); }
static void unused_drop(void *p,uint64_t v) { (void)p; (void)v; abort(); }
static void checked_context(void) {
	struct rnx_provider provider={NULL,unused_create,unused_read,unused_drop};
	struct rnx_response out;
	struct rnx_handles_request r={.version=2,.size=sizeof(r),.operation=RNX_OPEN,
		.provider=&provider,.output=&out};
	assert(rnx_engine_handles_v2(&r)==RNX_OK);
	r.operation=RNX_CLOSE;
	r.context=out.handle;
	r.provider=NULL;
	assert(rnx_engine_handles_v2(&r)==RNX_OK);
}
static void ordinary_context(void) {
	size_t written,needed;
	struct rnx_request r={.version=1,.size=sizeof(r),.path=(const uint8_t*)script,
		.path_len=strlen(script),.fuel=100000,.written=&written,.needed=&needed};
	/* Native execution is off: a lookup here must precede native checkout. */
	assert(rnx_engine_run_file_v1(&r)==RNX_OK);
}
static void *fresh(void *checked_first) {
	assert(reads==0);
	if (checked_first) checked_context(); else ordinary_context();
	assert(reads==1 && present==expected);
	if (checked_first) ordinary_context(); else checked_context();
	assert(reads==1);
	return NULL;
}
int main(int argc,char **argv) {
	assert(argc==3);
	script=argv[1];
	expected=atoi(argv[2]);
	pthread_t a,b;
	assert(!pthread_create(&a,NULL,fresh,NULL));
	assert(!pthread_create(&b,NULL,fresh,&expected));
	assert(!pthread_join(a,NULL) && !pthread_join(b,NULL));
}
