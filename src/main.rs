#![no_std]
#![no_main]

use panic_halt as _;

use nb::block;

use core::{borrow::BorrowMut, cell::{Cell, RefCell}};
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

static CLICK_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0_u32));
static IS_SHINING: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static SHINE_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0_u32));
const LED_TOGGLE_TO_STOP: u32= 6;
const TIM_DELAYMS: u32 = 500;
static ITERATIONS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static LAST_CLICK_ITERATION: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

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
    button.trigger_on_edge(&mut dp.EXTI, Edge::Falling);
    button.enable_interrupt(&mut dp.EXTI);

    timer.start(TIM_DELAYMS.millis()).unwrap();
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
    cortex_m::interrupt::free(|cs| {
        let last_click_iteration = LAST_CLICK_ITERATION.borrow(cs);
        let current_iteration = ITERATIONS.borrow(cs);
        let click_count = CLICK_COUNT.borrow(cs);
        click_count.set(click_count.get() + 1);

        if last_click_iteration.get() != 0 &&  current_iteration.get() - last_click_iteration.get() >= 4 {
            click_count.set(0);
        }

        if click_count.get() == 2 {
            IS_SHINING.borrow(cs).set(true);
            click_count.set(0);
        }
        LAST_CLICK_ITERATION.borrow(cs).set(ITERATIONS.borrow(cs).get());
        let mut button = G_BUTTON.borrow(cs).borrow_mut();
        button.as_mut().unwrap().clear_interrupt_pending_bit();
    });
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        if IS_SHINING.borrow(cs).get() {
            let mut led = G_LED.borrow(cs).borrow_mut();
            led.as_mut().unwrap().toggle();

            let shine_count = SHINE_COUNT.borrow(cs);
            shine_count.set(shine_count.get() + 1);

            if shine_count.get() >= LED_TOGGLE_TO_STOP {
                IS_SHINING.borrow(cs).set(false);
                shine_count.set(0);
            }
        }

        ITERATIONS.borrow(cs).set(ITERATIONS.borrow(cs).get() + 1);
        
        let mut timer = G_TIM.borrow(cs).borrow_mut();
        timer.as_mut().unwrap().clear_interrupt(Event::Update);
    });
}
