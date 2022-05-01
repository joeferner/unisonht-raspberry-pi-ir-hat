import { OpenApi, Plugin, PluginConfig, PluginFactory, UnisonHTServer, validateJson } from '@unisonht/unisonht';
import { IrHat } from 'raspberry-pi-ir-hat';
import { RaspberryPiIrHatPluginConfig } from './Config';
import { updateOpenApi } from './updateOpenApi';
import path from 'path';

export class RaspberryPiIrHatPluginFactory implements PluginFactory<RaspberryPiIrHatPluginConfig> {
    async createPlugin(
        server: UnisonHTServer,
        config: PluginConfig<RaspberryPiIrHatPluginConfig>,
    ): Promise<Plugin<RaspberryPiIrHatPluginConfig>> {
        return new RaspberryPiIrHatPlugin(server, config);
    }
}

export class RaspberryPiIrHatPlugin extends Plugin<RaspberryPiIrHatPluginConfig> {
    private readonly irHat: IrHat;

    constructor(server: UnisonHTServer, config: PluginConfig<RaspberryPiIrHatPluginConfig>) {
        super(server, config);

        validateJson('RaspberryPiIrHatPluginConfig', config.data, {
            sourcePath: path.join(__dirname, 'Config.ts'),
            tsconfigPath: path.join(__dirname, '../tsconfig.json'),
        });

        this.irHat = new IrHat({ path: config.data.irHatPath });
        this.irHat.rx.subscribe((data) => {
            this.debug('rx: %o', data);
        });
    }

    override updateOpenApi(openApi: OpenApi): void {
        super.updateOpenApi(openApi);
        updateOpenApi(openApi, this.apiUrlPrefix, this.openApiTags);
    }
}
