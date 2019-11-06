#![doc = "Peripheral access API for SOC microcontrollers (generated using svd2rust v0.16.1)\n\nYou can find an overview of the API [here].\n\n[here]: https://docs.rs/svd2rust/0.16.1/svd2rust/#peripheral-api"]
#![deny(missing_docs)]
#![deny(warnings)]
#![allow(non_camel_case_types)]
#![no_std]
extern crate bare_metal;
extern crate riscv;
#[cfg(feature = "rt")]
extern crate riscv_rt;
extern crate vcell;
use core::marker::PhantomData;
use core::ops::Deref;
#[doc(hidden)]
pub mod interrupt;
pub use self::interrupt::Interrupt;
#[allow(unused_imports)]
use generic::*;
#[doc = r"Common register and bit access and modify traits"]
pub mod generic;
#[doc = "RGB"]
pub struct RGB {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for RGB {}
impl RGB {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const rgb::RegisterBlock {
        0x8000_6800 as *const _
    }
}
impl Deref for RGB {
    type Target = rgb::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*RGB::ptr() }
    }
}
#[doc = "RGB"]
pub mod rgb;
#[doc = "PICORVSPI"]
pub struct PICORVSPI {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for PICORVSPI {}
impl PICORVSPI {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const picorvspi::RegisterBlock {
        0x8000_5000 as *const _
    }
}
impl Deref for PICORVSPI {
    type Target = picorvspi::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*PICORVSPI::ptr() }
    }
}
#[doc = "PICORVSPI"]
pub mod picorvspi;
#[doc = "I2C"]
pub struct I2C {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for I2C {}
impl I2C {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const i2c::RegisterBlock {
        0x8000_4800 as *const _
    }
}
impl Deref for I2C {
    type Target = i2c::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*I2C::ptr() }
    }
}
#[doc = "I2C"]
pub mod i2c;
#[doc = "REBOOT"]
pub struct REBOOT {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for REBOOT {}
impl REBOOT {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const reboot::RegisterBlock {
        0x8000_6000 as *const _
    }
}
impl Deref for REBOOT {
    type Target = reboot::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*REBOOT::ptr() }
    }
}
#[doc = "REBOOT"]
pub mod reboot;
#[doc = "TICKTIMER"]
pub struct TICKTIMER {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for TICKTIMER {}
impl TICKTIMER {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const ticktimer::RegisterBlock {
        0x8000_7800 as *const _
    }
}
impl Deref for TICKTIMER {
    type Target = ticktimer::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*TICKTIMER::ptr() }
    }
}
#[doc = "TICKTIMER"]
pub mod ticktimer;
#[doc = "CTRL"]
pub struct CTRL {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for CTRL {}
impl CTRL {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const ctrl::RegisterBlock {
        0x8000_0000 as *const _
    }
}
impl Deref for CTRL {
    type Target = ctrl::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*CTRL::ptr() }
    }
}
#[doc = "CTRL"]
pub mod ctrl;
#[doc = "MESSIBLE"]
pub struct MESSIBLE {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for MESSIBLE {}
impl MESSIBLE {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const messible::RegisterBlock {
        0x8000_5800 as *const _
    }
}
impl Deref for MESSIBLE {
    type Target = messible::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*MESSIBLE::ptr() }
    }
}
#[doc = "MESSIBLE"]
pub mod messible;
#[doc = "TIMER0"]
pub struct TIMER0 {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for TIMER0 {}
impl TIMER0 {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const timer0::RegisterBlock {
        0x8000_2800 as *const _
    }
}
impl Deref for TIMER0 {
    type Target = timer0::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*TIMER0::ptr() }
    }
}
#[doc = "TIMER0"]
pub mod timer0;
#[doc = "VERSION"]
pub struct VERSION {
    _marker: PhantomData<*const ()>,
}
unsafe impl Send for VERSION {}
impl VERSION {
    #[doc = r"Returns a pointer to the register block"]
    #[inline(always)]
    pub const fn ptr() -> *const version::RegisterBlock {
        0x8000_7000 as *const _
    }
}
impl Deref for VERSION {
    type Target = version::RegisterBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*VERSION::ptr() }
    }
}
#[doc = "VERSION"]
pub mod version;
#[no_mangle]
static mut DEVICE_PERIPHERALS: bool = false;
#[doc = r"All the peripherals"]
#[allow(non_snake_case)]
pub struct Peripherals {
    #[doc = "RGB"]
    pub RGB: RGB,
    #[doc = "PICORVSPI"]
    pub PICORVSPI: PICORVSPI,
    #[doc = "I2C"]
    pub I2C: I2C,
    #[doc = "REBOOT"]
    pub REBOOT: REBOOT,
    #[doc = "TICKTIMER"]
    pub TICKTIMER: TICKTIMER,
    #[doc = "CTRL"]
    pub CTRL: CTRL,
    #[doc = "MESSIBLE"]
    pub MESSIBLE: MESSIBLE,
    #[doc = "TIMER0"]
    pub TIMER0: TIMER0,
    #[doc = "VERSION"]
    pub VERSION: VERSION,
}
impl Peripherals {
    #[doc = r"Returns all the peripherals *once*"]
    #[inline]
    pub fn take() -> Option<Self> {
        riscv::interrupt::free(|_| {
            if unsafe { DEVICE_PERIPHERALS } {
                None
            } else {
                Some(unsafe { Peripherals::steal() })
            }
        })
    }
    #[doc = r"Unchecked version of `Peripherals::take`"]
    pub unsafe fn steal() -> Self {
        DEVICE_PERIPHERALS = true;
        Peripherals {
            RGB: RGB {
                _marker: PhantomData,
            },
            PICORVSPI: PICORVSPI {
                _marker: PhantomData,
            },
            I2C: I2C {
                _marker: PhantomData,
            },
            REBOOT: REBOOT {
                _marker: PhantomData,
            },
            TICKTIMER: TICKTIMER {
                _marker: PhantomData,
            },
            CTRL: CTRL {
                _marker: PhantomData,
            },
            MESSIBLE: MESSIBLE {
                _marker: PhantomData,
            },
            TIMER0: TIMER0 {
                _marker: PhantomData,
            },
            VERSION: VERSION {
                _marker: PhantomData,
            },
        }
    }
}
