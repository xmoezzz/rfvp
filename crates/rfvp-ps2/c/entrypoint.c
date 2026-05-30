#include "ps2sdk_backend.h"

int main(int argc, char **argv) {
    int status = rfvp_ps2_platform_init(argc, argv);
    if (status != RFVP_PS2_OK) {
        return status;
    }

    RawPs2Host host;
    status = rfvp_ps2_make_raw_host(&host);
    if (status == RFVP_PS2_OK) {
        status = rfvp_ps2_app_main(&host);
    }

    rfvp_ps2_platform_fini();
    return status;
}
