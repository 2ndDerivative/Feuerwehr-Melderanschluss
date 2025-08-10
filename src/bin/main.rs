#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::cell::RefCell;

use critical_section::Mutex;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Event, Input, InputConfig, Io, Pull};
use esp_hal::rtc_cntl::sleep::{TimerWakeupSource, WakeSource};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::{handler, main};
use log::info;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// https://www.funkhandel.com/mediafiles/Sonstiges/Bedienungsanleitung/bda-lgra.pdf

esp_bootloader_esp_idf::esp_app_desc!();

static IO_RELAIS_ORIGIN: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static ALARM: Mutex<RefCell<AlarmState>> = Mutex::new(RefCell::new(AlarmState::Armed));

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(interrupt_handler);

    let pull_pin = peripherals.GPIO7;
    let mut relais_pin = Input::new(pull_pin, InputConfig::default().with_pull(Pull::Up));
    critical_section::with(|cs| {
        relais_pin.listen(Event::FallingEdge);
        IO_RELAIS_ORIGIN.borrow_ref_mut(cs).replace(relais_pin);
    });

    let mut rtc = Rtc::new(peripherals.LPWR);
    info!("Starting to sleep for alarm");
    loop {
        critical_section::with(|cs| {
            let alarm = ALARM.borrow_ref_mut(cs);
            if let AlarmState::Ongoing = *alarm {
                info!("TRIGGERED ALARM");
                IO_RELAIS_ORIGIN.borrow_ref_mut(cs).as_mut().unwrap().unlisten();
                drop(alarm);
                let timer_wake = TimerWakeupSource::new(COOLDOWN);
                rtc.sleep_light(&[&timer_wake as &dyn WakeSource]);
                info!("Rearming!");
                IO_RELAIS_ORIGIN
                    .borrow_ref_mut(cs)
                    .as_mut()
                    .unwrap()
                    .listen(Event::FallingEdge);
            } else {
                drop(alarm);
                rtc.sleep_light(&[]);
            }
        });
    }
}
// Weißes Kabel Pin 3
// Rotes Kabel Pin 5
// Blaue Kabel Pin 2

#[handler]
fn interrupt_handler() {
    critical_section::with(|cs| {
        let mut relais = IO_RELAIS_ORIGIN.borrow_ref_mut(cs);
        if let Some(bo) = relais.as_mut() {
            bo.clear_interrupt();
            ALARM.replace(cs, AlarmState::Ongoing);
        };
    });
}

const COOLDOWN: core::time::Duration = core::time::Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
enum AlarmState {
    Armed,
    Ongoing,
}
