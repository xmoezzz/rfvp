#include "libogc_backend.h"

int main(int argc, char **argv) {
    int status = rfvp_wii_platform_init(argc, argv);
    if (status != RFVP_WII_OK) {
        return status;
    }

    RawWiiHost host;
    status = rfvp_wii_make_raw_host(&host);
    if (status == RFVP_WII_OK) {
        status = rfvp_wii_app_main(&host);
    }

    rfvp_wii_platform_fini();
    return status;
}
