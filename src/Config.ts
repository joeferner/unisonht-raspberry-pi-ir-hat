import { RawIrHatSignal } from 'raspberry-pi-ir-hat';

export interface RaspberryPiIrHatPluginConfig {
    irHatPath: string;
}

export interface RaspberryPiIrHatDeviceConfig {
    buttons: RaspberryPiIrHatDeviceConfigButton[];
}

export interface RaspberryPiIrHatDeviceConfigButton extends RawIrHatSignal {
    name: string;
}
