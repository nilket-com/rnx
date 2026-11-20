#define _GNU_SOURCE
#include "../engine_image.h"
#include <assert.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

struct invocation {
	struct rnx_request r;
	size_t written, needed;
	uint8_t buffer[4096];
	unsigned calls;
	int offset;
	pthread_barrier_t *overlap;
};
static int64_t apply(void *ctx, int64_t x) {
	struct invocation *i = ctx;
	++i->calls;
	if (i->overlap) pthread_barrier_wait(i->overlap);
	assert(rnx_engine_run_file_v1(&i->r) == RNX_INVALID);
	return x + i->offset;
}
static void prepare(struct invocation *i, const char *file, int offset) {
	*i = (struct invocation){.offset = offset, .written=888, .needed=999};
	i->r = (struct rnx_request){ .version=1, .size=sizeof(i->r), .fuel=100000,
		.path=(const uint8_t *)file, .path_len=strlen(file), .context=i, .apply=apply,
		.buffer=i->buffer, .capacity=sizeof(i->buffer), .written=&i->written, .needed=&i->needed };
}
static void bad(struct invocation *i, struct rnx_request request) {
	assert(rnx_engine_run_file_v1(&request) == RNX_INVALID);
	assert(!i->calls && i->written==888 && i->needed==999);
}
static void *execute(void *ctx) {
	struct invocation *i=ctx;
	assert(rnx_engine_run_file_v1(&i->r)==RNX_OK && i->calls==1 && i->needed==0 && i->written==0);
	return NULL;
}
int main(int argc, char **argv) {
	assert(argc==4);
	struct invocation first, second;
	prepare(&first, argv[1], 2);
	prepare(&second, argv[1], 3);
	long page = sysconf(_SC_PAGESIZE);
	assert(page>0);
	uint8_t *mem=mmap(NULL,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
	assert(mem!=MAP_FAILED && !mprotect(mem+page,page,PROT_NONE));
	uint32_t *header=(uint32_t *)(mem+page)-2;
	header[0]=1; header[1]=8;
	assert(rnx_engine_run_file_v1((void *)header)==RNX_INVALID);
	header[0]=2; header[1]=88;
	assert(rnx_engine_run_file_v1((void *)header)==RNX_INVALID);
	assert(!munmap(mem,2*page));
	struct rnx_request r=first.r;
	r.flags=4; bad(&first,r);
	r=first.r; r.apply=NULL; bad(&first,r);
	r=first.r; r.path=(uint8_t *)"bad\0path"; r.path_len=8; bad(&first,r);
	r=first.r; r.path=(uint8_t *)"\xff"; r.path_len=1; bad(&first,r);
	r=first.r; r.buffer=NULL; bad(&first,r);
	r=first.r; r.fuel=UINT64_MAX; bad(&first,r);
	execute(&first); execute(&second);
	prepare(&first,argv[1],7); prepare(&second,argv[1],9);
	pthread_barrier_t barrier;
	assert(!pthread_barrier_init(&barrier,NULL,2));
	first.overlap=second.overlap=&barrier;
	pthread_t worker;
	assert(!pthread_create(&worker,NULL,execute,&first));
	execute(&second);
	assert(!pthread_join(worker,NULL) && !pthread_barrier_destroy(&barrier));
	prepare(&first,argv[2],2);
	assert(rnx_engine_run_file_v1(&first.r)==RNX_REFUSED && first.calls==0 && first.needed>0);
	prepare(&first,argv[3],2);
	assert(rnx_engine_run_file_v1(&first.r)==RNX_FAILED && first.calls==1 && first.written==first.needed);
	assert(memmem(first.buffer,first.written,"runtime error",13));
	size_t full=first.needed;
	for (unsigned cap=0; cap<2; ++cap) {
		prepare(&second,argv[3],3);
		second.r.capacity=cap; second.r.buffer=cap ? second.buffer : NULL;
		assert(rnx_engine_run_file_v1(&second.r)==RNX_FAILED && second.calls==1);
		assert(second.needed==full && second.written==cap && !memcmp(first.buffer,second.buffer,cap));
	}
}
