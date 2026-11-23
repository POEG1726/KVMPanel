/* Generated from api/schema/ws.schema.json by `npm run gen:ws`. Do not edit. */

export type ClientMessage = Key | MouseAbs | MouseRel | Wheel | Paste | Shortcut | Ping;
/**
 * Bit mask: 1 left, 2 right, 4 middle, 8 back, 16 forward
 */
export type Buttons = number;
export type ServerMessage = Status | UsbReenumerated | KvmRestarting | Error | Pong;

export interface WsMessages {
  client?: ClientMessage;
  server?: ServerMessage;
}
export interface Key {
  t: "key";
  /**
   * KeyboardEvent.code, e.g. KeyA, CapsLock
   */
  code: string;
  down: boolean;
}
export interface MouseAbs {
  t: "mouse_abs";
  x: number;
  y: number;
  buttons: Buttons;
}
export interface MouseRel {
  t: "mouse_rel";
  dx: number;
  dy: number;
  buttons: Buttons;
}
export interface Wheel {
  t: "wheel";
  dy: number;
}
export interface Paste {
  t: "paste";
  /**
   * At most 64 KiB of UTF-8 bytes (checked by the server)
   */
  text: string;
}
/**
 * Press keys in order, then release in reverse, e.g. [ControlLeft, AltLeft, Delete]
 */
export interface Shortcut {
  t: "shortcut";
  /**
   * @minItems 1
   * @maxItems 8
   */
  keys:
    | [string]
    | [string, string]
    | [string, string, string]
    | [string, string, string, string]
    | [string, string, string, string, string]
    | [string, string, string, string, string, string]
    | [string, string, string, string, string, string, string]
    | [string, string, string, string, string, string, string, string];
}
export interface Ping {
  t: "ping";
}
/**
 * Sent on connect, on every change and at least once per second
 */
export interface Status {
  t: "status";
  hdmi: {
    signal: boolean;
    width: number;
    height: number;
    fps: number;
  };
  stream: {
    fps: number;
    kbps: number;
  };
  power: {
    on: boolean;
  };
  locks: {
    num: boolean;
    caps: boolean;
    scroll: boolean;
  };
  sessions: {
    user: string;
    ip: string;
    /**
     * Unix ms
     */
    since: number;
    view_only: boolean;
  }[];
  media: {
    /**
     * File name of the mounted image
     */
    mounted: string | null;
    mode: "cdrom" | "disk" | null;
  };
}
export interface UsbReenumerated {
  t: "usb_reenumerated";
}
/**
 * Sent right before kvmd restarts; the server then closes with code 1012
 */
export interface KvmRestarting {
  t: "kvm_restarting";
}
export interface Error {
  t: "error";
  /**
   * unmappable: paste contains characters the configured keyboard layout cannot type; the whole message is rejected
   */
  error: "bad_message" | "forbidden" | "unmappable";
}
export interface Pong {
  t: "pong";
}

export type StatusMessage = Status;
