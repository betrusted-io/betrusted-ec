// This file is Copyright (c) 2013-2014 Sebastien Bourdeauducq <sb@m-labs.hk>
// This file is Copyright (c) 2019 Gabriel L. Somlo <gsomlo@gmail.com>
// License: BSD


#include <generated/csr.h>
#include <irq.h>
#include <uart.h>
#include <stdio.h>

void isr(void);
void isr(void)
{
	unsigned int irqs;

	irqs = irq_pending() & irq_getmask();

}
