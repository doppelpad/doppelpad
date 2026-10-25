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
      en: 'Xbox and other XInput controllers. XInput hides the Elite paddles and trigger rumble for now.',
      de: 'Xbox- und andere XInput-Controller. Die Elite-Paddles und Trigger-Rumble verdeckt XInput vorerst.',
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
      en: 'Also NSO and Switch-style wired pads, with gyro and rumble. Windows via raw HID (awaiting hardware tests), Linux through hid-nintendo.',
      de: 'Auch NSO- und Switch-artige Kabel-Pads, mit Gyro und Rumble. Windows über Raw HID (Hardwaretest steht aus), Linux über hid-nintendo.',
    },
  },
  {
    name: { en: 'Joy-Con', de: 'Joy-Con' },
    front: 'joycon',
    back: 'joyconBack',
    family: 'joycon',
    note: {
      en: 'Paired automatically, or sideways on their own (awaiting hardware tests on Windows).',
      de: 'Automatisch als Paar oder einzeln quer (Hardwaretest unter Windows steht aus).',
    },
  },
  {
    name: { en: 'Switch 2 Pro Controller', de: 'Switch 2 Pro Controller' },
    front: 'switch2pro',
    back: 'switch2proBack',
    family: 'nintendo',
    note: {
      en: 'Over USB and Bluetooth on Windows, with the GL and GR buttons (awaiting hardware tests; over Bluetooth Windows 10 reads it only about 17 times a second, against 250 over USB). Linux is planned.',
      de: 'Über USB und Bluetooth unter Windows, mit den Tasten GL und GR (Hardwaretest steht aus; über Bluetooth liest Windows 10 ihn nur etwa 17-mal pro Sekunde, über USB 250-mal). Linux ist geplant.',
    },
  },
  {
    name: { en: 'GameCube', de: 'GameCube' },
    front: 'gamecube',
    back: 'gamecubeBack',
    family: 'gamecube',
    note: {
      en: 'The Switch 2 GameCube controller over USB, and GameCube controllers on the official USB adapter or its Wii U-mode clones (Windows, awaiting hardware tests).',
      de: 'Der GameCube-Controller der Switch 2 über USB und GameCube-Controller am offiziellen USB-Adapter oder seinen Klonen im Wii-U-Modus (Windows, Hardwaretest steht aus).',
    },
  },
  {
    name: { en: 'Other gamepads', de: 'Andere Gamepads' },
    front: 'generic',
    back: 'genericBack',
    family: 'generic',
    note: {
      en: 'Other HID gamepads and joysticks: a wizard identifies their buttons once (Windows). Linux reads them through evdev.',
      de: 'Andere HID-Gamepads und Joysticks: ein Assistent erkennt ihre Tasten einmalig (Windows). Linux liest sie über evdev.',
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
