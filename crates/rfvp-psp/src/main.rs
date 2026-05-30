#![no_std]
#![no_main]

psp::module!("rfvp-psp", 1, 0);

fn psp_main() {
    psp::enable_home_button();
    let status = rfvp_psp::platform::run();
    if status != 0 {
        psp::dprintln!("rfvp-psp exited with status {}", status);
    }
}
