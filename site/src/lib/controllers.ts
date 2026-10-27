// The controllers on the landing page, with the app's drawings and how far their support goes
// (after docs/ROADMAP.md, "Input controllers").

import type { DeviceId } from '../../../gui/src/assets/devices/layouts';
import { controlNames, type Family, type Lang } from './controls';

/** A controller's tab: its drawings, the names of their controls and how far support goes. */
export interface Controller {
  name: string;
  front: DeviceId;
  back: DeviceId;
  /** Control → name, as the app shows it. */
  names: Record<string, string>;
  note: string;
}

interface Pad {
  name: Record<Lang, string>;
  front: DeviceId;
  back: DeviceId;
  family: Family;
  note: Record<Lang, string>;
}

const PADS: Pad[] = [
  {
    name: { en: 'Xbox', de: 'Xbox' },
    front: 'xbox',
    back: 'xboxBack',
    family: 'xbox',
    note: {
      en: 'Xbox and other XInput controllers. XInput hides the Elite paddles and trigger rumble for now. On macOS over Bluetooth, as a generic pad; not over USB yet.',
      de: 'Xbox- und andere XInput-Controller. Die Elite-Paddles und Trigger-Rumble verdeckt XInput vorerst. Unter macOS über Bluetooth als generisches Gamepad, über USB noch nicht.',
    },
  },
  {
    name: { en: 'DualSense / DualShock 4', de: 'DualSense / DualShock 4' },
    front: 'dualsense',
    back: 'dualsenseBack',
    family: 'playstation',
    note: {
      en: 'USB and Bluetooth, with gyro, touchpad, light bar and adaptive triggers (those still await hardware tests).',
      de: 'USB und Bluetooth, mit Gyro, Touchpad, Lichtleiste und adaptiven Triggern (deren Hardwaretest steht noch aus).',
    },
  },
  {
    name: { en: 'DualSense Edge', de: 'DualSense Edge' },
    front: 'dualsenseEdge',
    back: 'dualsenseEdgeBack',
    family: 'playstation',
    note: {
      en: 'Everything the DualSense has, plus the Fn and back buttons.',
      de: 'Alles, was der DualSense hat, dazu die Fn- und Rücktasten.',
    },
  },
  {
    name: { en: 'Switch Pro Controller', de: 'Switch Pro Controller' },
    front: 'switchpro',
    back: 'switchproBack',
    family: 'nintendo',
    note: {
      en: 'Also NSO and Switch-style wired pads, with gyro and rumble. Windows via raw HID (awaiting hardware tests), Linux through hid-nintendo, macOS through IOKit HID (the Pro Controller tried over USB and Bluetooth).',
      de: 'Auch NSO- und Switch-artige Kabel-Pads, mit Gyro und Rumble. Windows über Raw HID (Hardwaretest steht aus), Linux über hid-nintendo, macOS über IOKit HID (der Pro Controller über USB und Bluetooth ausprobiert).',
    },
  },
  {
    name: { en: 'Joy-Con', de: 'Joy-Con' },
    front: 'joycon',
    back: 'joyconBack',
    family: 'joycon',
    note: {
      en: 'Paired automatically, or sideways on their own (awaiting hardware tests).',
      de: 'Automatisch als Paar oder einzeln quer (Hardwaretest steht aus).',
    },
  },
  {
    name: { en: 'Switch 2 Pro Controller', de: 'Switch 2 Pro Controller' },
    front: 'switch2pro',
    back: 'switch2proBack',
    family: 'nintendo',
    note: {
      en: 'Over USB and Bluetooth on Windows, Linux and macOS, with the GL and GR buttons (on Windows over USB still awaiting hardware tests). Over USB it sends 250 times a second; over Bluetooth the system sets the pace: about 33 times on macOS, about 20 on Linux, and on Windows 10 unevenly, at times 17 and at times about 45 on average.',
      de: 'Über USB und Bluetooth unter Windows, Linux und macOS, mit den Tasten GL und GR (unter Windows über USB steht der Hardwaretest noch aus). Über USB sendet er 250-mal pro Sekunde; über Bluetooth gibt das System den Takt vor: unter macOS etwa 33-mal, unter Linux etwa 20-mal und unter Windows 10 ungleichmäßig, mal 17-, mal im Schnitt etwa 45-mal.',
    },
  },
  {
    name: { en: 'GameCube', de: 'GameCube' },
    front: 'gamecube',
    back: 'gamecubeBack',
    family: 'gamecube',
    note: {
      en: 'The Switch 2 GameCube controller over USB (Windows, Linux and macOS), and GameCube controllers on the official USB adapter or its Wii U-mode clones (Windows). Both await hardware tests.',
      de: 'Der GameCube-Controller der Switch 2 über USB (Windows, Linux und macOS) und GameCube-Controller am offiziellen USB-Adapter oder seinen Klonen im Wii-U-Modus (Windows). Bei beiden steht der Hardwaretest aus.',
    },
  },
  {
    name: { en: 'Other gamepads', de: 'Andere Gamepads' },
    front: 'generic',
    back: 'genericBack',
    family: 'generic',
    note: {
      en: 'Other HID gamepads and joysticks: a wizard identifies their buttons once (Windows and macOS). Linux reads them through evdev.',
      de: 'Andere HID-Gamepads und Joysticks: ein Assistent erkennt ihre Tasten einmalig (Windows und macOS). Linux liest sie über evdev.',
    },
  },
];

export const controllers = Object.fromEntries(
  (['en', 'de'] as const).map((lang) => [
    lang,
    PADS.map(({ name, front, back, family, note }) => ({
      name: name[lang],
      front,
      back,
      names: controlNames([front, back], family, lang),
      note: note[lang],
    })),
  ]),
) as Record<Lang, Controller[]>;
