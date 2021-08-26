//! Driver for LSM6DS3 inertial measurement unit (IMU).
//!
//! The LSM6DS3 has an accelerometer and gyroscope along with many sensor
//! co-processor features intended for use with a popular phone operating
//! system -- we don't need all that. This driver only supports basic
//! accelerometer functionality and the tap detection interrupt feature.
//!
use crate::hal_i2c::Hardi2c;
use crate::hal_time::delay_ms;

/*
I2C control register and initialization sequence notes from STM app note
AN5130, "LSM6DS3TR-C: always-on 3D accelerometer and 3D gyroscope"

Chip Power On Initialization Sequence
=====================================
1. Automatic loading of "trim" config values takes 15ms
2. Serial IO mode defaults to Mode 1 (I2C on, SPI off)
3. Accelerometer (XL) and gyroscope (G) power states default to Power-Down
4. Measurement endianness defaults to little-endian (CTRL3_C[BLE] = 0)
5. Interrupt active polarity defaults to Active High (CTRL3_C[H_LACTIVE] = 0)
6. Latching of Interrupts defaults to off (TAP_CFG[LIR] = 0)
7. I2C clock speed 100kHz-400kHz
8. I2C address is b110_1011=0x6B (last bit 1 because SA0 is wired to Vdd)

Abbreviations
=============
XL:  Accelerometer
G    Gyroscope
BW:  BandWidth (of filter, related to ODR)
DA:  Data Available (status indication bit)
DUR: Duration
FS:  Full Scale magnitude
IA:  Interrupt Available (maybe? used in int. status registers)
LPF: Low Pass Filter
ODR: Output Data Rate (Hz)
THS: Threshold (related to FS)

Reg name     Addr  Fields
===========  ====  ============================================================
CTRL1_XL     0x10  {ODR_XL[3:0], FS_XL[1:0], LPF1_BW_SEL, BW0_XL}
CTRL2_G      0x11  {ODR_G[3:0], FS_G[1:0], FS_125, 0}
CTRL3_C      0x12  {BOOT, BDU, H_LACTIVE, PP_OD, SIM, IF_INC, BLE, SW_RESET}
TAP_SRC      0x1C  {0, TAP_IA, SINGLE_TAP, DOUBLE_TAP, TAP_SIGN, X_TAP, Y_TAP, Z_TAP}
WAKE_UP_THS  0x5B  {SINGLE_DOUBLE_TAP, 0, WK_THS[5:0]}
TAP_CFG      0x58  {INTERRUPTS_ENABLE, INACT_EN[1:0], SLOPE_FDS, TAP_X_EN, TAP_Y_EN, TAP_Z_EN, LIR}
TAP_THS_6D   0x59  {D4D_EN, SIXD_THS[1:0], TAP_THS[4:0]}
INT_DUR2     0x5A  {DUR[3:0], QUIET[1:0], SHOCK[1:0]}
MD1_CFG      0x5E  {INT1_INACT, INT1_SINGLE, INT1_WU, INT1_FF, INT1_DOUBLE, INT1_6D, INT1_TILT, INT1_TIMER}

Block Data Update (BDU)
=======================
Set CTRL3_C[BDU] = 1 to enable Block Data Update. BDU enables latching for
MSB/LSB register pairs such that reading one register of a pair latches the
other until both have been read.

SLOPE_FDS
=========
TAP_CFG[SLOPE_FDS] selects trigger source for accelerometer wake and inactivity
modes. Default value of SLOPE_FDS=0 uses slope detection filter as trigger.

Activity Detection Modes INACT_EN[1:0]
======================================
0b00: Inactivity detection is off (no automatic sampling rate reduction)
0b01: Inactive -> ODR_XL=12.5Hz, gyro unchanged
0b10: Inactive -> ODR_XL=12.5Hz, gyro sleep
0b11: Inactive -> ODR_XL=12.5Hz, gyro power-down

Example of Configuring Tap Detection
====================================
1. CTRL1_XL    = 0x60  // ODR_XL:416Hz, FS_XL:2g, LPF1_BW=ODR/2=208Hz, BW0_XL:400Hz
2. TAP_CFG     = 0x83  // InterruptsEn:On, InactiveEn:Off, TriggerSrc:Slope, TriggerAxis:Z, LatchIR:On
3. TAP_THS_6D  = 0x89  // b1_00_01001: 4dDetect:Off, 6dTHS:80°, TapTHS:562.5mg (LSB=FS_XL/(2^5); 9*(2g/32)=562.5mg)
4. INT_DUR2    = 0x06  // b0000_01_10: DoubleTapGapDur:16*ODR (LSB=32*ODR), Quiet:1*(4/ODR)=9.6ms, Shock:2*(8/ODR)=38.5ms
5. WAKE_UP_THS = 0x00  // b0_0_000000: SingleDouble:Single, 0, WakeTHS:0g (LSB=FS_XL/(2^6))
6. MD1_CFG     = 0x40  // b0_1_0_0_0_0_0_0: INT1 pin driven by single-tap interrupt (and no others)

*/

const IMU_I2C_ADDR: u8 = 0x6B;
const WHO_AM_I: u8 = 0x0F;
const CTRL1_XL: u8 = 0x10;
const CTRL2_G: u8 = 0x11;
const CTRL3_C: u8 = 0x12;
const TAP_SRC: u8 = 0x1C;
const WAKE_UP_THS: u8 = 0x5B;
const TAP_CFG: u8 = 0x58;
const TAP_THS_6D: u8 = 0x59;
const INT_DUR2: u8 = 0x5A;
const MD1_CFG: u8 = 0x5E;
const OUTX_L_XL: u8 = 0x28;
const OUTX_H_XL: u8 = 0x29;
const OUTY_L_XL: u8 = 0x2A;
const OUTY_H_XL: u8 = 0x2B;
const OUTZ_L_XL: u8 = 0x2C;
const OUTZ_H_XL: u8 = 0x2D;

const I2C_TIMEOUT_MS: u32 = 2;

/// Write value to the specified IMU register address
fn i2c_w(i2c: &mut Hardi2c, reg_addr: u8, reg_val: u8, err_tag: u8) -> Result<u8, u8> {
    let txbuf: [u8; 2] = [reg_addr, reg_val];
    // This loop is a safer version of the `while i2c... != 0 {}` pattern used
    // elsewhere. Hard limit on retries ensures this function will return promptly
    // and without risk of deadlock in the event of an I2C bus fault. Expected
    // result is `return Ok` on first pass.
    for _ in 0..3 {
        if i2c.i2c_controller(IMU_I2C_ADDR, Some(&txbuf), None, I2C_TIMEOUT_MS) == 0 {
            return Ok(0);
        }
    }
    // Reaching this line may indicate a hardware fault (I2C bus, PnR timing, etc.)
    return Err(err_tag);
}

/// Read u8 value from the specified IMU register address.
/// Note that although this function is for reading a single-byte register, the Hardi2c module
/// requires reads of at least 2 bytes as a workaround for an issue with the ICE40UP5K I2C block.
fn i2c_r(i2c: &mut Hardi2c, reg_addr: u8, err_tag: u8) -> Result<u8, u8> {
    let txbuf: [u8; 1] = [reg_addr];
    let mut rxbuf: [u8; 2] = [0, 0];
    for _ in 0..3 {
        if i2c.i2c_controller(IMU_I2C_ADDR, Some(&txbuf), Some(&mut rxbuf), I2C_TIMEOUT_MS) == 0 {
            return Ok(rxbuf[0]);
        }
    }
    // Reaching this line may indicate a hardware fault (I2C bus, PnR timing, etc.)
    return Err(err_tag);
}

pub struct Imu {}

impl Imu {
    /// Preform IMU boot and software reset procedures to ensure known config (18ms delay)
    fn boot_and_reset(mut i2c: &mut Hardi2c) -> Result<u8, u8> {
        i2c_w(&mut i2c, CTRL2_G, 0x00, 0x1)?; // Gyro -> power-down mode
        i2c_w(&mut i2c, CTRL1_XL, 0x60, 0x2)?; // Accelerometer -> high-performance mode
        i2c_w(&mut i2c, CTRL3_C, 0x80, 0x3)?; // Initiate BOOT (takes 15ms)
        delay_ms(16);
        i2c_w(&mut i2c, CTRL3_C, 0x01, 0x4)?; // Initiate SW_RESET (takes 50µs)
        delay_ms(2);
        Ok(0)
    }

    /// Initialize the IMU for single-tap detection, returning value of WHO_AM_I register on success
    pub fn init(mut i2c: &mut Hardi2c) -> Result<u8, u8> {
        Self::boot_and_reset(&mut i2c)?;
        // CTRL1_XL = ODR_XL:416Hz, FS_XL:2g, LPF1_BW:208Hz, BW0_XL:400Hz
        i2c_w(&mut i2c, CTRL1_XL, 0x60, 0x5)?;
        // CTRL3_C = BlockDataUpdate:On
        i2c_w(&mut i2c, CTRL3_C, 0x40, 0x6)?;
        // TAP_CFG = InterruptsEn:On, InactiveEn:Off, TriggerSrc:Slope, TriggerAxis:Z, LatchIR:On
        i2c_w(&mut i2c, TAP_CFG, 0x83, 0x7)?;
        // TAP_THS_6D = b1_00_01001: 4dDetect:Off, 6dTHS:80°, TapTHS:562.5mg (LSB=FS_XL/(2^5); 9*(2g/32)=562.5mg)
        i2c_w(&mut i2c, TAP_THS_6D, 0x89, 0x8)?;
        // INT_DUR2 = b0000_10_10: DoubleTapGapDur:16*ODR (LSB=32*ODR), Quiet:3*(4/ODR)=29ms, Shock:2*(8/ODR)=39ms
        i2c_w(&mut i2c, INT_DUR2, 0x0E, 0x9)?;
        // WAKE_UP_THS = b0_0_000000: SingleDouble:Single, 0, WakeTHS:0g (LSB=FS_XL/(2^6))
        i2c_w(&mut i2c, WAKE_UP_THS, 0x00, 0xA)?;
        // MD1_CFG = b0_1_0_0_0_0_0_0: INT1 pin driven by single-tap interrupt (and no others)
        i2c_w(&mut i2c, MD1_CFG, 0x40, 0xB)?;
        Self::get_who_am_i(&mut i2c)
    }

    /// Check the WHO_AM_I register which should contain 0x6A
    pub fn get_who_am_i(mut i2c: &mut Hardi2c) -> Result<u8, u8> {
        i2c_r(&mut i2c, WHO_AM_I, 0xC)
    }

    /// Get current accelerometer X axis measurement
    pub fn get_accel_x(mut i2c: &mut Hardi2c) -> Result<u16, u8> {
        let lsb = i2c_r(&mut i2c, OUTX_L_XL, 0x0D)?;
        let msb = i2c_r(&mut i2c, OUTX_H_XL, 0x0E)?;
        Ok(u16::from_le_bytes([lsb, msb]))
    }

    /// Get current accelerometer Y axis measurement
    pub fn get_accel_y(mut i2c: &mut Hardi2c) -> Result<u16, u8> {
        let lsb = i2c_r(&mut i2c, OUTY_L_XL, 0x0F)?;
        let msb = i2c_r(&mut i2c, OUTY_H_XL, 0x10)?;
        Ok(u16::from_le_bytes([lsb, msb]))
    }

    /// Get current accelerometer Z axis measurement
    pub fn get_accel_z(mut i2c: &mut Hardi2c) -> Result<u16, u8> {
        let lsb = i2c_r(&mut i2c, OUTZ_L_XL, 0x11)?;
        let msb = i2c_r(&mut i2c, OUTZ_H_XL, 0x12)?;
        Ok(u16::from_le_bytes([lsb, msb]))
    }

    /// Returns true result if there is a latched single-tap interrupt
    pub fn get_single_tap(mut i2c: &mut Hardi2c) -> Result<bool, u8> {
        const TAP_IA: u8 = 0x40;
        const SINGLE_TAP: u8 = 0x20;
        const MASK: u8 = TAP_IA | SINGLE_TAP;
        let ts = i2c_r(&mut i2c, TAP_SRC, 0x13)?;
        let tap_happened = (ts & MASK) == MASK;
        Ok(tap_happened)
    }
}
