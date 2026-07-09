/* 设备运行时验证：dlopen 我们的 Rust .so，调用 escore_probe，打印派生地址。
 * 用 NDK 的 armv7a-linux-androideabi19-clang 编译，push 到 /data/local/tmp 与 .so 同目录运行。
 * 期望：ret=42 addr=bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu */
#include <dlfcn.h>
#include <stdio.h>

typedef int (*probe_fn)(unsigned char *, unsigned long);

int main(void) {
    void *h = dlopen("/data/local/tmp/libesp_signer_core.so", RTLD_NOW);
    if (!h) {
        printf("dlopen FAIL: %s\n", dlerror());
        return 1;
    }
    probe_fn f = (probe_fn)dlsym(h, "escore_probe");
    if (!f) {
        printf("dlsym FAIL: %s\n", dlerror());
        return 2;
    }
    unsigned char buf[128] = {0};
    int n = f(buf, sizeof(buf));
    printf("ret=%d addr=%s\n", n, buf);
    return 0;
}
