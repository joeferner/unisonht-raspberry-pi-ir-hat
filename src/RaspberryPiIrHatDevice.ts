import { Device, DeviceConfig, DeviceFactory, PowerState, UnisonHTServer, validateJson } from '@unisonht/unisonht';
import path from 'path';
import { RaspberryPiIrHatDeviceConfig } from './Config';

export class RaspberryPiIrHatDeviceFactory implements DeviceFactory<RaspberryPiIrHatDeviceConfig> {
    async createDevice(
        server: UnisonHTServer,
        config: DeviceConfig<RaspberryPiIrHatDeviceConfig>,
    ): Promise<RaspberryPiIrHatDevice> {
        return new RaspberryPiIrHatDevice(server, config);
    }
}

export class RaspberryPiIrHatDevice extends Device<RaspberryPiIrHatDeviceConfig> {
    constructor(server: UnisonHTServer, config: DeviceConfig<RaspberryPiIrHatDeviceConfig>) {
        super(server, config);

        validateJson('RaspberryPiIrHatDeviceConfig', config.data, {
            sourcePath: path.join(__dirname, 'Config.ts'),
            tsconfigPath: path.join(__dirname, '../tsconfig.json'),
        });
    }

    handleButtonPress(button: string): Promise<void> {
        throw new Error('Method not implemented.');
    }

    async switchMode(oldModeId: string | undefined, newModeId: string): Promise<void> {
        // TODO support configuration to send IR codes on mode switch
    }

    async switchInput(inputName: string): Promise<void> {
        // TODO support configuration to send IR codes on input switch
    }

    async getPowerState(): Promise<PowerState> {
        return PowerState.ON;
    }

    get buttons(): string[] {
        return this.config.data.buttons.map((b) => b.name);
    }
}
