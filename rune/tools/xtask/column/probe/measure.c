#include <valgrind/callgrind.h>
#include <linux/perf_event.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <stdint.h>
#include <string.h>
#include <fcntl.h>
#include <stdlib.h>
#include <time.h>
static uint64_t thermal_before, thermal_after, began_ns, ended_ns;
static uint64_t thermal(void) {
    int fd=open("/sys/devices/system/cpu/cpu2/thermal_throttle/core_throttle_count",O_RDONLY|O_CLOEXEC);
    if(fd<0)return UINT64_MAX;
    char buf[64];ssize_t n=read(fd,buf,sizeof(buf)-1);close(fd);
    if(n<=0)return UINT64_MAX;
    buf[n]=0;char *end;unsigned long long v=strtoull(buf,&end,10);
    if(end==buf)return UINT64_MAX;
    return v;
}
static uint64_t ns(void){struct timespec t;if(clock_gettime(CLOCK_MONOTONIC,&t))return 0;return(uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec;}
int step_cycles_open(void) {
    struct perf_event_attr a;
    memset(&a,0,sizeof(a));
    a.type=PERF_TYPE_HARDWARE;a.size=sizeof(a);a.config=PERF_COUNT_HW_CPU_CYCLES;
    a.disabled=1;a.exclude_kernel=1;a.exclude_hv=1;
    a.read_format=PERF_FORMAT_TOTAL_TIME_ENABLED|PERF_FORMAT_TOTAL_TIME_RUNNING;
    return syscall(__NR_perf_event_open,&a,0,-1,-1,0);
}
void step_begin(int fd) {
    thermal_before=thermal();
    CALLGRIND_START_INSTRUMENTATION;
    CALLGRIND_ZERO_STATS;
    if(fd>=0){ioctl(fd,PERF_EVENT_IOC_RESET,0);ioctl(fd,PERF_EVENT_IOC_ENABLE,0);}
    began_ns=ns();
}
void step_end(int fd, uint64_t *out) {
    ended_ns=ns();
    if(fd>=0){ioctl(fd,PERF_EVENT_IOC_DISABLE,0);if(read(fd,out,24)!=24)out[0]=out[1]=out[2]=0;}
    CALLGRIND_DUMP_STATS;
    CALLGRIND_STOP_INSTRUMENTATION;
    thermal_after=thermal();
}
void step_close(int fd) {if(fd>=0)close(fd);}
uint64_t step_thermal_before(void){return thermal_before;}
uint64_t step_thermal_after(void){return thermal_after;}
uint64_t step_elapsed_ns(void){return ended_ns>=began_ns?ended_ns-began_ns:0;}
