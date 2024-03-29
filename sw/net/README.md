# Net Crate

This crate handles the portion of the EC network stack that can reasonably be
written with `#[forbid(unsafe_code)]`. Mostly `net` does packet building and
parsing with u8 buffers containing Ethernet II frames (fixed length 14 byte
header, as opposed to variable length 802.3) as the central data structure.

For people reading this who are already experts in Rust, network programming,
or embedded systems, please forgive the potential explanations of things you
already know. The main idea here is to help people from a variety of
backgrounds get up to speed with sufficient tools and background knowledge to
read and understand the code of this crate.


## Reading RFCs

References to "RFC" in comments of this crate refer to IETF RFC standards
documents describing Internet protocols unless otherwise noted. IETF RFCs are,
for the most part, unrelated to Rust language RFCs.

Many of the variable names and hardcoded byte offsets in this crate correspond
to packet header diagrams in IETF RFC documents. When you see code comments
that mention RFC numbers, you can assume that code was written with an editor
window open next to browser tabs with the relevant RFCs. Reading the code may
make more sense if you do the same thing.


## Using Serial Port Debug Logging

Code in this crate sends log messages to the UP5K UART (serial port) which you
can access using a Precursor JTAG debug cable connected to the Precursor Debug
Hat for Raspberry Pi. To learn about the hardware setup, read the wiki docs on
using the JTAG cable to flash gateware and firmware binaries:
https://github.com/betrusted-io/betrusted-wiki/wiki

Most of the code in this crate was written with two SSH shells open to a
Raspberry Pi with a Debug Hat and JTAG debug cable. One shell runs `screen` as
a serial terminal emulator using the `start_screen_up5k.sh` script from the
[betrusted-scripts repo](https://github.com/betrusted-io/betrusted-scripts).

The other shell alternates between these scripts from betrusted-scripts:

1. `uart_fpga.sh`: switch Debug Hat serial port MUX to the XC7S (SoC, Xous)
    UART for using key injection to type shellchat commands such as `wlan
    setssid ...`, `wlan setpass ...`, and `wlan join`. Doing this once after
    you boot or reset the SoC gets you set up to repeat the commands using the
    F3 and Enter keys (F-keys in shellchat let you repeat recent commands).

2. `uart_up5k.sh`: switch Debug Hat serial port MUX back to UP5K (EC) for
   watching debug log output from the net crate

3. `config_up5k.sh`: flash EC bitstream and firmware after compiling new code

At the time I'm writing this, in September 2021, it generally works fine to
leave `screen` open while switching the Debug Hat UART mux between the EC and
SoC UARTs. If you enable wishbone-bridge for gdb debugging, it will get more
complicated.


## On Hex-Formatted u8 Error Codes

This section attempts to explain the theory and rationale behind the perhaps
cryptic looking error handling pattern, based on `u8` error code literals, used
in this crate.

The key idea is that errors will show up on the serial log as a string like,
`UniqueStr 5A`, which you can use with a multi-file search against the source
code of this repo.

Searching for `UniqueStr` will lead to the arm of a match expression in a high
level function, such as `Err(e) => loghexln!(... "UniqueStr ", e),`. The match
will be on a `Result` from calling an intermediate level function. Searching
that intermediate function for the `5A` hex literal from the debug log should
lead to a unique match, like `return Err(0x5A);` or `foo(..., 0x5a)?;`, that
identifies the error's origin.

For rationale... Space on the EC is tight. To achieve reasonable runtime
instruction fetch speed, the EC copies its code from flash to RAM at boot, then
executes code from RAM. The code, heap, and stack must all fit in 128KB. There
is not much space to spend on code for generating fancy error messages.

This error handling strategy is a compromise between having tolerable human
readable serial port error messages and keeping the code small. Using "{:X}"
consistently for number formatting limits the amount of `core::fmt` code that
needs to be linked. Also, base-16 conversion can be done without all the
divisions needed for base-10 conversions. The difference matters because the
EC's RV32I CPU core is slow at division.

The big advantage of using a brute force strategy like this is that it's
expedient and efficient. Due to its simplicity, the error handling plumbing
itself is unlikely to generate problems. Relying on a serial debug log and text
editor search features means the error reporting code can be small and simple.

In practice, the error handling mostly looks like this:

1. High level functions call intermediate level functions, and `match` patterns
   like this to convert from `Err(_)` return values to formatted log messages
   on the debug serial port:
   ```
   fn high_level_func(...) {
       match intermediate_func(...) {
           Ok(...) => ...,
           Err(e) => loghexln!(LL::Debug, "UniqueStr ", e),
       };
   }
   ```

2. Intermediate functions return `Result<_, u8>` and use a sequence of `if`
   expressions, `match` expressions, and function calls, each associated with a
   unique `u8` literal. The `u8` error codes get returned in an `Err()` for
   problems in the related code. Calls to lower-level functions use Rust's
   question mark operator to propagate errors up the call stack (see Rust
   `Result` docs). It looks like this:

   ```
   fn intermediate_func(...) -> Result<(), u8> {
       if ... {
           return Err(0x01);
       }
       let ... = low_level_func1(..., 0x02)?;  // 0x02 is error code argument
       let ... = low_level_func2(..., 0x03)?;
       ...
       Ok(())
   }
   ```

3. Low-level functions take an error code argument like `e: u8` and return
   a `Result<_, u8>`, where returning `Err(e)` means the function failed:
   ```
   fn low_level_func1(..., e: u8) -> Result<..., u8> {
       if ... {
           return Err(e);
       }
       match ... {
           ...
           _ => return Err(e),
       };
       Ok(...)
   }
   ```


## On Using Bitwise Operations Instead of Division Operations

This crate sometimes uses bitwise operations (`>>`, `&`) instead of division or
modulo division.

If you are unfamiliar with equivalence between bitwise operations and division,
the key idea to understand is that right-shifting a base-2 number is equivalent
to taking the quotient from dividing that number by 2. If you shift right by
more than one digit, that is equivalent to dividing by 2 raised to the power of
however many digits you shift.

For example, in base-2, seventeen is 0b10001. If you shift 3 digits off the
right end of seventeen, you get 0b10, which is equivalent to 2 in base-10. So,
`17 >> 3 = 2`. That result is the same as calculating the quotient of dividing
17 by 8: `17 / (2**3) = 17 / 8 = 2`.

If you want to calculate the remainder of an integer division using a divisor
of `2**n`, you can do a bitwise AND against `(2**n)-1`.

For example, the remainder of 17 divided by 8 is 1, which can be calculated as
`17 % (2**3) = 17 % 8 = 1`. You can also get the same result using bitwise AND
like this, `17 & ((2**3)-1) = 17 & 7 = 1`. So, doing `& 7` is equivalent to
doing `% 8`. They both mean that you want to keep just the lowest three bits,
which is equivalent to taking the remainder from an integer division by 8.


## On Quotas and Bounded Loops

This section is an attempt to explain the philosophical basis for some unusual
code patterns used in this crate (at least compared to web apps and such).

This crate is written with the goals of being robust and including built in
diagnostics. Ideally, it should not panic, it should not fail under heavy load,
and it should degrade gracefully in the presence of hardware faults. Probably
this code will fall short, because most code usually does. But, those are still
the goals.

Degrading gracefully includes things like diagnosing assembly faults during
factory tests, or logging missing peripherals when bringing up a hardware
simulation environment. Toward that end, the primary techniques are avoiding
blocking the main event loop and being sure to return error codes. Not blocking
the event loop means avoiding panics and avoiding getting stuck in infinite
loops. As long as functions return promptly to return control flow to the event
loop, then there will be an opportunity to log and report any error codes they
return.

One of the ways this crate attempts to pursue that goal is by making sure that
loops are always bounded. In practice, that means estimating the maximum
resonable number of possible loop iterations, then using `for` rather than
`while` or `loop`. To learn more about the rationale for using bounded loops,
try searching for "The Power of 10: Rules for Developing Safety-Critical Code".

Another method to make things robust is to plan for always having an upper
limit on numeric quantities. Memory allocations are bounded. Retry timeouts are
bounded. Integer math is bounded (using saturate or wrap instead of panic on
overflow). The method of picking limits can use calculations, tests and
measurements, rough estimating, wild guessing, or whatever. The important thing
is that the code checks for limits in controlled ways that can detect and
report errors without interrupting the control flow of the main event loop.


## On Using Wireshark

If you want to modify this crate, or understand it well, you probably should
set up a test LAN with Wireshark for packet capture and protocol analysis.

For people with experience using Wireshark and test networks... You probably
already know better ways to do this, so please feel free to ignore the rest of
this section.

For people who are new to network programming and who need a simple starting
point for setting up a test network, something about like this will probably
work:

Equipment:
- Small *managed* Ethernet switch with at least 3 ports and a "port mirroring"
  feature
- 2 wifi routers with Ethernet LAN ports (one needs "bridge mode" or an option
  to disable its DHCP server)
- 3 Ethernet cables
- Computer with a free Ethernet port and Wireshark

Procedure:

1. Configure one wifi router to bridge packets between its wifi and Ethernet
   LAN interfaces (DHCP should be *off*). Set this router up with the SSID and
   PSK passphrase that you will connect to from Precursor.

2. Configure the other wifi router to act as a DHCP server and gateway for your
   test LAN. Set this router up with a different SSID and use it for connecting
   whatever other devices you want your Precursor to talk to.

3. Plug the two routers (use LAN port, *not* WAN port) and your Wireshark
   computer into the managed switch.

4. Configure the managed switch's port mirroring feature to mirror packets
   *from* the port for the bridge mode wifi router *to* the port for your
   Wireshark computer. With port mirroring configured, all the packets passing
   to and from your bridge mode router should be visible to Wireshark. With
   Precursor on one router and the device it's talking to on the other router,
   Wireshark should see the conversation.

5. Use Wireshark in the normal way. You might find it helpful to use a display
   filter like "not arp and not mdns and not dns".
