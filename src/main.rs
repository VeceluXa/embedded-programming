#![no_std]
#![no_main]

use panic_halt as _;

use nb::block;

use core::cell::{Cell, RefCell};
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use stm32f1xx_hal::{
    gpio::{self, Edge, ExtiPin, Input, Output, PushPull},
    pac::{self, interrupt, TIM2}, 
    prelude::*,
    timer::{CounterMs, Event}};

type ButtonPin = gpio::PC13<Input>;
type LedPin = gpio::PA5<Output>;
static G_BUTTON: Mutex<RefCell<Option<ButtonPin>>> = Mutex::new(RefCell::new(None));
static G_TIM: Mutex<RefCell<Option<CounterMs<TIM2>>>> = Mutex::new(RefCell::new(None));
static G_LED: Mutex<RefCell<Option<LedPin>>> = Mutex::new(RefCell::new(None));
static G_DELAYMS: Mutex<Cell<u32>> = Mutex::new(Cell::new(2000_u32));

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    
    let mut gpioa = dp.GPIOA.split();
    let mut gpioc = dp.GPIOC.split();
    
    let mut led = gpioa.pa5.into_push_pull_output(&mut gpioa.crl);
    let mut button = gpioc.pc13;//.into_floating_input(&mut gpioc.crh);

    let mut flash = dp.FLASH.constrain();
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.freeze(&mut flash.acr);
    let mut timer = dp.TIM2.counter_ms(&clocks);

    //  we need to enable fking interruptions:
    //  1. Firt of all we need to enable bit PRIMASK of Cortex-M processor. Fortunately, this shit
    //     is turned on by default.
    //  2. Then we need to enable interruptions on peripheral level:
    //     Alternate function input/output
    let mut afio = dp.AFIO.constrain(); 
    button.make_interrupt_source(&mut afio);
    //     External interrupt/event controller
    button.trigger_on_edge(&mut dp.EXTI, Edge::Rising);
    button.enable_interrupt(&mut dp.EXTI);

    timer.start(2000.millis()).unwrap();
    timer.listen(Event::Update);
    //  3. NVIC - interruption controller, we need to tell him which interruption we want to use
    //     and we need to unmask it
    unsafe {
        cortex_m::peripheral::NVIC::unmask(interrupt::EXTI15_10);
        cortex_m::peripheral::NVIC::unmask(interrupt::TIM2);
    }

    cortex_m::interrupt::free(|cs| {
        G_BUTTON.borrow(cs).replace(Some(button));
        G_TIM.borrow(cs).replace(Some(timer));
        G_LED.borrow(cs).replace(Some(led));
    });

     loop {
        cortex_m::asm::wfi();
         }
}

#[interrupt]
fn EXTI15_10() {
// When Button interrupt happens three things need to be done
    // 1) Adjust Global Delay Variable
    // 2) Update Timer with new Global Delay value
    // 3) Clear Button Pending Interrupt

    // Start a Critical Section
    cortex_m::interrupt::free(|cs| {
        // Obtain Access to Delay Global Data and Adjust Delay
        G_DELAYMS
            .borrow(cs)
            .set(G_DELAYMS.borrow(cs).get() - 500_u32);

        // Reset delay value if it drops below 500 milliseconds
        if G_DELAYMS.borrow(cs).get() < 500_u32 {
            G_DELAYMS.borrow(cs).set(2000_u32);
        }

        // Obtain access to global timer
        let mut timer = G_TIM.borrow(cs).borrow_mut();

        // Adjust and start timer with updated delay value
        timer
            .as_mut()
            .unwrap()
            .start(G_DELAYMS.borrow(cs).get().millis())
            .unwrap();

        // Obtain access to Global Button Peripheral and Clear Interrupt Pending Flag
        let mut button = G_BUTTON.borrow(cs).borrow_mut();
        button.as_mut().unwrap().clear_interrupt_pending_bit();
    });
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let mut led = G_LED.borrow(cs).borrow_mut();
        led.as_mut().unwrap().toggle();

        let mut timer = G_TIM.borrow(cs).borrow_mut();
        timer.as_mut().unwrap().clear_interrupt(Event::Update);
    });
}
