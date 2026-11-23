/*
 * stc8g1k08a.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#include "board.h"
#include "power.h"
#include "protocol.h"

/* STC8G Series Manual §8.3. The 08A has Timer 0 and Timer 1 only. */
__sfr __at(0x87) PCON;
__sfr __at(0x88) TCON;
__sfr __at(0x89) TMOD;
__sfr __at(0x8a) TL0;
__sfr __at(0x8b) TL1;
__sfr __at(0x8c) TH0;
__sfr __at(0x8d) TH1;
__sfr __at(0x8e) AUXR;
__sfr __at(0x98) SCON;
__sfr __at(0x99) SBUF;
__sfr __at(0xa2) P_SW1;
__sfr __at(0xa8) IE;
__sfr __at(0xb0) P3;
__sfr __at(0xb1) P3M1;
__sfr __at(0xb2) P3M0;
__sfr __at(0xba) P_SW2;
__sfr __at(0xbc) ADC_CONTR;
__sfr __at(0xbd) ADC_RES;
__sfr __at(0xbe) ADC_RESL;
__sfr __at(0xc1) WDT_CONTR;
__sfr __at(0xc2) IAP_DATA;
__sfr __at(0xc3) IAP_ADDRH;
__sfr __at(0xc4) IAP_ADDRL;
__sfr __at(0xc5) IAP_CMD;
__sfr __at(0xc6) IAP_TRIG;
__sfr __at(0xc7) IAP_CONTR;
__sfr __at(0xc8) P5;
__sfr __at(0xc9) P5M1;
__sfr __at(0xca) P5M0;
__sfr __at(0xde) ADCCFG;
__sfr __at(0xf5) IAP_TPS;
__sfr __at(0xff) RSTCFG;

__sbit __at(0x98) RI;
__sbit __at(0x99) TI;
__sbit __at(0xaf) EA;
__sbit __at(0xb1) UART_TX;
__sbit __at(0xb3) MAIN_EN;
__sbit __at(0xcc) RESET_EN;
__sbit __at(0xcd) PD_EN;

#define XREG(address) (*(__xdata volatile uint8_t *)(address))
#define CKSEL  XREG(0xfe00)
#define CLKDIV XREG(0xfe01)
#define P3PU   XREG(0xfe13)
#define P3IE   XREG(0xfe33)
#define ADCTIM XREG(0xfea8)

#define IRC_HZ 11059200UL
#define CLOCK_DIVISOR 6
#define SYS_HZ (IRC_HZ / CLOCK_DIVISOR)
#define BAUD 9600UL
#define T0_RELOAD (65536UL - SYS_HZ * TICK_MS / 1000)
#define T1_RELOAD (65536UL - SYS_HZ / (4 * BAUD))
#define IAP_WAIT ((SYS_HZ + 500000UL) / 1000000UL)

#define ADC_POWER 0x80
#define ADC_START 0x40
#define ADC_DONE  0x20
#define ADC_KEY   2

#define WDT_ENABLE 0x20
#define WDT_CLEAR  0x10
#define WDT_PS_DIV8 0x02

#if SYS_HZ * TICK_MS % 1000 || SYS_HZ % (4 * BAUD)
#error Timer reload values must be integral
#endif

/* The T113 updater validates this code-space image trailer before asserting HOLD. */
static const __code __at(STC_IMAGE_END) uint8_t image_id[] = {
    'K', 'V', 'S', '8', STC_APP_VERSION & 0xff, STC_APP_VERSION >> 8
};

static volatile uint16_t ticks;
static uint16_t adc, rx_at;
static uint8_t adc_fresh, rx_used, tx_sent;
static __idata uint8_t rx[FRAME_SIZE], tx[FRAME_SIZE];

/** @brief Set safe latches before enabling their outputs, before C RAM init. */
unsigned char _sdcc_external_startup(void)
{
    RESET_EN = 1;
    PD_EN = 0;
    MAIN_EN = 0;
    UART_TX = 1;
    /* PxM1/PxM0: 01 push-pull, 10 input-only. P3.0 and P3.2 are inputs. */
    P3M1 = 0x05;
    P3M0 = 0x0a;
    P5M1 = 0;
    P5M0 = 0x30;
    return 0;
}

/** @brief Count exact 5 ms hardware reloads. */
void timer0_isr(void) __interrupt(1) __using(1)
{
    ++ticks;
}

/** @brief Read the timer without tearing the 16-bit value. */
static uint16_t clock_ticks(void)
{
    uint16_t now;
    EA = 0;
    now = ticks;
    EA = 1;
    return now;
}

/** @brief Initialize the IRC divider, UART1 and continually powered ADC2. */
static void peripherals(void)
{
    /* P_SW2.EAXS makes FE00..FEFF XFRs visible to MOVX. CKSEL=0 is HIRC. */
    P_SW2 |= 0x80;
    CLKDIV = CLOCK_DIVISOR;
    CKSEL = 0;
    P3PU &= (uint8_t)~0x05; /* Disable the RX and ADC pull-ups. */
    P3IE &= (uint8_t)~0x04; /* Remove the digital input buffer on ADC2. */
    ADCTIM = 0x3f;         /* Setup 1, hold 2, sample 32 ADC clocks. */
    P_SW2 &= (uint8_t)~0x80;
    ADCCFG = 0x2f; /* Right-aligned 10 bits; SYSCLK / 2 / 16 = 57.6 kHz. */
    ADC_CONTR = ADC_POWER | ADC_KEY;

    P_SW1 &= 0x3f; /* UART1 default P3.0 RX and P3.1 TX. */
    PCON &= 0x7f;  /* SMOD=0. */
    SCON = 0x50;   /* Mode 1, reception enabled, no UART IRQ. */
    TMOD = 0;      /* Both timers use STC 16-bit auto-reload mode 0. */
    AUXR = 0xc0;  /* T0 and T1 at 1T, UART1 uses T1. */
    TL0 = (uint8_t)T0_RELOAD;
    TH0 = (uint8_t)(T0_RELOAD >> 8);
    TL1 = (uint8_t)T1_RELOAD;
    TH1 = (uint8_t)(T1_RELOAD >> 8);
    IAP_TPS = IAP_WAIT;
}

/** @brief Drive the exact board pin for a sequencer output. */
void board_output(uint8_t output, uint8_t high)
{
    if (output == OUT_MAIN)
        MAIN_EN = high;
    else if (output == OUT_PD)
        PD_EN = high;
    else
        RESET_EN = high;
}

/** @brief Run one protected IAP operation with a contiguous trigger sequence. */
static uint8_t flash_op(uint8_t cmd, uint16_t address, uint8_t value)
{
    uint8_t irq = EA;
    uint8_t success;
    EA = 0;
    IAP_CONTR = 0x80;
    IAP_CMD = cmd;
    IAP_ADDRH = (uint8_t)(address >> 8);
    IAP_ADDRL = (uint8_t)address;
    IAP_DATA = value;
    IAP_TRIG = 0x5a;
    IAP_TRIG = 0xa5;
    __asm nop __endasm;
    success = !(IAP_CONTR & 0x10);
    IAP_CONTR = 0;
    IAP_CMD = 0;
    IAP_TRIG = 0;
    IAP_ADDRH = 0x80;
    IAP_ADDRL = 0;
    EA = irq;
    return success;
}

/** @brief Read a Data Flash byte using its zero-based IAP address. */
uint8_t board_flash_read(uint16_t address, uint8_t *value)
{
    uint8_t success = flash_op(1, address, 0xff);
    *value = IAP_DATA;
    return success;
}

/** @brief Program one byte in the previously erased part of the journal. */
uint8_t board_flash_write(uint16_t address, uint8_t value)
{
    return flash_op(2, address, value);
}

/** @brief Erase a 512-byte Data Flash sector. */
uint8_t board_flash_erase(uint16_t address)
{
    return flash_op(3, address, 0xff);
}

/** @brief Require P5.4 selected as GPIO in the ISP hardware options. */
uint8_t board_isp_allowed(void)
{
    return !(RSTCFG & 0x10);
}

/** @brief Software reset to ROM ISP (SWBS | SWRST). */
void board_enter_isp(void)
{
    EA = 0;
    IAP_CONTR = 0x60;
}

/** @brief Take the latest 10-bit result and immediately restart ADC2. */
static void adc_service(void)
{
    if (ADC_CONTR & ADC_DONE) {
        adc = ((uint16_t)(ADC_RES & 3) << 8) | ADC_RESL;
        adc_fresh = 1;
        ADC_CONTR = ADC_POWER | ADC_START | ADC_KEY;
    }
}

/** @brief Poll at most one RX and one TX byte. A stalled transmitter never waits. */
static void uart_service(uint16_t now)
{
    uint8_t byte, cmd;
    uint16_t value;
    if (TI) {
        TI = 0;
        if (tx_sent < FRAME_SIZE)
            SBUF = tx[tx_sent++];
        else
            tx_sent = FRAME_SIZE + 1;
    }
    if (!RI)
        return;
    byte = SBUF;
    RI = 0;
    if (rx_used && (uint16_t)(now - rx_at) >= 5)
        rx_used = 0;
    rx_at = now;
    if (!rx_used && byte != FRAME_SOF0)
        return;
    if (rx_used == 1 && byte != FRAME_SOF1) {
        rx_used = byte == FRAME_SOF0;
        return;
    }
    rx[rx_used++] = byte;
    if (rx_used != FRAME_SIZE)
        return;
    rx_used = 0;
    if (tx_sent <= FRAME_SIZE || !stc_frame_valid(rx))
        return;
    cmd = rx[2];
    value = stc_value(rx);
    if (cmd == CMD_STATUS)
        value = power_status();
    else if (cmd == CMD_VERSION)
        value = STC_APP_VERSION;
    else if (!power_command(cmd, value, now))
        rx[2] = REJECT;
    rx[2] |= REPLY;
    stc_set_value(rx, value);
    for (byte = 0; byte < FRAME_SIZE; ++byte)
        tx[byte] = rx[byte];
    tx_sent = 1;
    SBUF = tx[0];
}

/** @brief Run the rail, ADC, journal and UART services without idle/sleep. */
void main(void)
{
    uint16_t now, previous = 0;
    tx_sent = FRAME_SIZE + 1;
    peripherals();
    /* STC8G §6.2.1: start WDT and clear its counter, 12 × 32768 × 8 / SYS_HZ ≈ 1.71 s. */
    WDT_CONTR = WDT_ENABLE | WDT_CLEAR | WDT_PS_DIV8;
    power_init(0);
    /* The power-on journal scan gives the ADC more than its startup settling time. */
    ADC_CONTR = ADC_POWER | ADC_START | ADC_KEY;
    TCON = 0x50; /* Run Timer 0 and Timer 1. */
    IE = 0x82;   /* Only Timer 0 interrupt. */
    for (;;) {
        adc_service();
        now = clock_ticks();
        if (now != previous) {
            previous = now;
            power_tick(now, adc, adc_fresh);
            adc_fresh = 0;
            power_save_step(now);
            /* Clear the WDT only after a new timer tick has completed foreground work. */
            WDT_CONTR |= WDT_CLEAR;
        }
        uart_service(now);
    }
}
