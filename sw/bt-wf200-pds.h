/*
 * This file tells the WF200 what circuits are around it, and how it should interact with them.
 * It is specific to each board. In this case, Betrusted.
 */
 
/*
 * The PDS is used to enter configuration data.
 *
 * This file is a template that shows all elements that exist.
 * Sections PROG_PINS_CFG and HIF_PINS_CFG contain hardware default values (i.e. after reset, before PDS download).
 *
 * Please do not use it with real hardware: copy it and update it to fit to your own hardware.
 * It should then be processed by the pds_compress script to generate the compressed
 * data to be provided either to the Linux driver or to the Full MAC driver.
 *
 * Unneeded sections can be commented (like in ANSI C) or deleted.
 */

#include "imports/wfx-firmware/PDS/definitions.in"

// PDS API version
HEADER: {
    VERSION_MAJOR: 4,
    VERSION_MINOR: 0
},

/**********************/
/* Pins configuration */
/**********************/

// Configuration of programmable pins
PROG_PINS_CFG: {
    // For each programmable pin in this section
    // SLEW_RATE sets the maximum slew rate on the pin. Type: integer value between 0 and 6 (6=max drive strength)
    // PULL_UP_DOWN allows to add a pull-up or pull-down on the pad. Type: enum = 'none', 'down', 'up'
    // SLEEP_CFG allows to add a pull-up or pull-down on the pad while in sleep mode. Type: enum = 'none', 'down', 'up', 'maintain'
    //          for the case of pads used as gpio it is also possible to maintain the current driven gpio value
    // PIN_MODE allows to configure the pin in tristate, functional mode or gpio. Type: enum = 'tri', 'func', 'gpio'
    // GPIO_ID allows to assign a GPIO_ID to a given pin when configured as gpio. Type = char must be an UPPER case letter
    GPIO_FEM_1: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: A
    },
    GPIO_FEM_2: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: B
    },
    GPIO_FEM_3: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: C
    },
    GPIO_FEM_4: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: D
    },
    GPIO_FEM_5: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: E
    },
    GPIO_FEM_6: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: F
    },
    GPIO_PDET: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: G
    },
    GPIO_PTA_TX_CONF: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: H
    },
    GPIO_PTA_RF_ACT: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: I
    },
    GPIO_PTA_STATUS: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: J
    },
    GPIO_PTA_FREQ: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: tri,
        GPIO_ID: K
    },
    GPIO_WUP: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: func,
        GPIO_ID: L
    },
    GPIO_WIRQ: {
        SLEW_RATE: 4,
        PULL_UP_DOWN: none,
        SLEEP_CFG: none,
        PIN_MODE: func,
        GPIO_ID: M
    }
},

// Configuration of Host Interface (HIF) pins
HIF_PINS_CFG: {
    // For each HIF pin in this section
    // SLEW_RATE sets the maximum slew rate on the pin. Type: integer value between 0 and 6 (6=max drive strength)
    // SLEEP_CFG for SDIO_D0_SPI_MISO allows to add a pull-up or pull-down on the pad while in sleep mode. Type: enum = 'none', 'down', 'up'
    SDIO_CLK_SPI_CLK: {
        SLEW_RATE: 4
    },
    SDIO_CMD_SPI_MOSI: {
        SLEW_RATE: 4
    },
    SDIO_D0_SPI_MISO: {
        SLEW_RATE: 6,
        SLEEP_CFG: none
    },
    SDIO_D1_SPI_WIRQ: {
        SLEW_RATE: 3
    },
    SDIO_D2_HIF_SEL: {
        SLEW_RATE: 3
    },
    SDIO_D3_SPI_CSN: {
        SLEW_RATE: 3
    }
},

/***********************/
/* Clock configuration */
/***********************/

// capacitance load target is 10pF; Cstray ~4pF so CL for each pin should be 10-4 = 6pF * 2 = 12pF
HF_CLK: {
    // XTAL_CFG allows to fine tune XTAL Oscillator frequency
    XTAL_CFG: {
        // CTUNE_FIX allows to set a high value capacitance on both XTAL_I and XTAL_O. Type: integer between 0 and 3 (default = 3)
        CTUNE_FIX: 3,  // 9pF each pin
        // CTUNE_XI allows to fine tune the capacitor on pin XTAL_I. Type: integer between 0 and 255 (default = 140)
        CTUNE_XI: 38,  // 80fF per LSB, 38 = ~3pF
        // CTUNE_XO similar to CTUNE_XI for XTAL_O pin
        CTUNE_XO: 38
    },
    // XTAL_SHARED indicates if the crystal is shared with another IC and thus must be kept active during sleep. Type: enum = 'no', 'yes'
    XTAL_SHARED: no,
    // XTAL_TEMP_COMP activates or deactivates the XTAL temperature compensation. Type: enum = 'enabled', 'disabled'
    XTAL_TEMP_COMP: disabled
},

/*********************/
/* FEM configuration */
/*********************/

FEM_CFG: {
    FEM_CTRL_PINS: {
        // defines state of FEM pins 1 to 6 depending on priority given to COEX and WLAN, and WLAN interface TX/RX state
        // notes:
        // - each bit indicates the pin level for each state
        // - pin FEM_4 is not present because it is the PA_enable signal
        // - keys with prefix WLAN_ONLY are the only used if PTA is not enabled
        //                      .-- FEM_6
        //                      | .- FEM_5
        //                      | |.- FEM_3
        //                      | ||.- FEM_2
        //                      | |||.- FEM_1
        //                      | ||||
        WLAN_ONLY_IDLE:       0b0_0000,  // FEM control signals for WLAN when not transmitting nor receiving
        WLAN_ONLY_RX:         0b0_0000,  // FEM control signals for WLAN when receiving
        WLAN_ONLY_TX:         0b0_0000,  // FEM control signals for WLAN when transmitting
        COEX_ONLY:            0b0_0000,  // FEM control signals to provide the antenna to coexisting RF
        COMBINED_WLAN_IDLE:   0b0_0000,  // control signals to set FEM in Rx for both WLAN and COEX (WLAN not receiving)
        COMBINED_WLAN_RX:     0b0_0000   // control signals to set FEM in Rx for both WLAN and COEX (WLAN actually receiving)
    },

    FEM_TIMINGS: {
        // define related timings on FEM signals
        // Delays are in 12.5ns units
        // Format integer
        TX_EN_DELAY:  16,  // max 65535, default value 16 => 0.2 us
        TX_DIS_DELAY: 13,  // max 255, default value 13 => 0.1625 us
        PA_EN_DELAY:  130, // max 255, default value 130 => 1.625 us
        PA_DIS_DELAY: 5,   // max 255, default value 5 => 0.0625 us
        RX_EN_DELAY:  0,   // max 255, default value 0
        RX_DIS_DELAY: 0    // max 255, default value 0
    }
},

/***********************/
/* Power configuration */
/***********************/

// Tx power-related information
RF_POWER_CFG: {
    // Designate the RF port affected by the following configurations
    // Thus it must always be the first element of this section (else it is ignored)
    // RF_PORT_1, RF_PORT_2, RF_PORT_BOTH (default)
    RF_PORT: RF_PORT_BOTH,

    // The max Tx power value in quarters of dBm. Type: signed integer between -128 and 127 (range in dBm: [-32; 31.75])
    // It is used to limit the Tx power. Thus a value larger than 80 (default) does not make sense.
    MAX_OUTPUT_POWER_QDBM: 80,

    // Front-end loss (loss between the chip and the antenna) in quarters of dB.
    // Type: signed integer between -128 and 127 (range in dB: [-32; 31.75])
    // This value must be positive when the front end attenuates the signal and negative when it amplifies it.
    // Values on Rx path and Tx path can be different thus 2 values are provided here
    // Default values are 0 for both
    FRONT_END_LOSS_TX_QDB: 0,
    FRONT_END_LOSS_RX_QDB: 0,

    // Backoff vs. Modulation Group vs Channel
    // CHANNEL_NUMBER: Designate a channel number (an integer) or a range of channel numbers (an array)
    // e.g. CHANNEL_NUMBER: [3, 9]: Channels from 3 to 9
    //
    // Each backoff value sets an attenuation for a group of modulations.
    // BACKOFF_VAL is given in quarters of dB. Type: unsigned integer. Covered range in dB: [0; 63.75]
    // A modulation group designates a subset of modulations:
    //   *  MOD_GROUP_0: B_1Mbps, B_2Mbps, B_5.5Mbps, B_11Mbps
    //   *  MOD_GROUP_1: G_6Mbps, G_9Mbps, G_12Mbps, N_MCS0,  N_MCS1,
    //   *  MOD_GROUP_2: G_18Mbps, G_24Mbps, N_MCS2, N_MCS3,
    //   *  MOD_GROUP_3: G_36Mbps, G_48Mbps, N_MCS4, N_MCS5
    //   *  MOD_GROUP_4: G_54Mbps, N_MCS6
    //   *  MOD_GROUP_5: N_MCS7
    //   BACKOFF_VAL: [MOD_GROUP_0, ..., MOD_GROUP_5]
    //   Default value: BACKOFF_QDB: [{ CHANNEL_NUMBER: [1,14], BACKOFF_VAL: [0, 0, 0, 0, 0 ,0] } ]
    BACKOFF_QDB: [
        {
            CHANNEL_NUMBER: 1,
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: 2,
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: [3, 9],
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: 10,
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: 11,
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: [12, 13],
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        },
        {
            CHANNEL_NUMBER: 14,
            BACKOFF_VAL: [0, 0, 0, 0, 0 ,0]
        }
    ]
},

/********************/
/* RF configuration */
/********************/

RF_ANTENNA_SEL_DIV_CFG: {
    //
    // Antenna selection: allows to select the RF port used. Can be different for Tx and Rx (FEM case)
    //   - TX1_RX1: RF_1 used (default)
    //   - TX2_RX2: RF_2 used
    //   - TX1_RX2: Tx on RF_1 and Rx on RF_2
    //   - TX2_RX1: Tx on RF_2 and Rx on RF_1
    //   - TX12_RX12: antenna diversity case: both Tx and Rx on the same RF port which is automatically selected (requires DIVERSITY to be set)
    //
    RF_PORTS: TX1_RX1, // for initial testing. also should try TX12_RX12 with both antennae installed

    //
    // Diversity control mode:
    //   - OFF (default)
    //   - INTERNAL: requires RF_PORTS to be set to TX12_RX12
    //
    DIVERSITY: OFF
},
