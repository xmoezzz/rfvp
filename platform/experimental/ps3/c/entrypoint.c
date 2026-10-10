#include "psl1ght_backend.h"

int main(int argc, char **argv) {
    int status = rfvp_ps3_platform_init(argc, argv);
    if (status != RFVP_PS3_OK) {
        return status;
    }

    status = rfvp_ps3_app_main();

    rfvp_ps3_platform_fini();
    return status;
}
