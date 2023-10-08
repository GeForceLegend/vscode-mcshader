import * as vscode from 'vscode'
import * as lc from 'vscode-languageclient/node'
import * as commands from './commands'
import { log } from './log'
import * as path from 'path'
import * as notification from './notification'

export class Extension {
    private statusBarItem: vscode.StatusBarItem | null = null
    private extensionContext: vscode.ExtensionContext | null = null
    private languageClient: lc.LanguageClient

    readonly extensionID = 'GeForceLegend.vscode-mcshader'

    updateStatus = (icon: string, text: string) => {
        this.statusBarItem?.dispose()
        this.statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left)
        this.statusBarItem.text = icon + ' [mc-shader] ' + text
        this.statusBarItem.show()
    }

    clearStatus = () => {
        this.statusBarItem?.dispose()
    }

    onStatusChange = (params: {
        status: 'loading' | 'ready' | 'failed' | 'clear'
        message: string
        icon: string
    }) => {
        switch (params.status) {
            case 'loading':
            case 'ready':
            case 'failed':
                this.updateStatus(params.icon, params.message)
                break
            case 'clear':
                this.clearStatus()
                break
        }
    }

    get context(): vscode.ExtensionContext {
        return this.extensionContext
    }

    get client(): lc.LanguageClient {
        return this.languageClient
    }

    public activate = async (context: vscode.ExtensionContext) => {
        this.extensionContext = context

        log.info('starting language server...')

        const serverPath = process.env['MCSHADER_DEBUG'] ?
            this.extensionContext.asAbsolutePath(path.join('server', 'target', 'debug', 'vscode-mcshader.exe')) :
            this.extensionContext.asAbsolutePath(path.join('server', 'vscode-mcshader.exe'))

        const server: lc.Executable = {
            command: serverPath,
            options: { env: { 'RUST_BACKTRACE': '1', ...process.env } }
        }
        const serverOption = {
            run: server,
            debug: server
        }
        this.languageClient = new lc.LanguageClient(
            'mcshader',
            'Minecraft Shaders LSP - Server',
            serverOption,
            {
                diagnosticCollectionName: 'mcshader',
                documentSelector: [{ scheme: 'file', language: 'glsl' }],
                synchronize: {
                    configurationSection: 'mcshader',
                },
            }
        )
        log.info('running with binary at path:\n\t', serverPath)
        this.updateStatus('$(loading~spin)', 'Starting...')
        await this.languageClient.start()

        this.extensionContext.subscriptions.push(...commands.commandList(this))
        this.extensionContext.subscriptions.push(this.languageClient.onNotification(notification.StatusUpdateNoticationMethod, this.onStatusChange))

        log.info('language server started!')
    }

    deactivate = async () => {
        await this.languageClient.stop()
        this.context.subscriptions?.forEach((disposable) => disposable.dispose())
    }
}

export const activate = async (context: vscode.ExtensionContext) => {
    try {
        new Extension().activate(context)
    } catch (e) {
        log.error(`failed to activate extension: ${e}`)
        throw (e)
    }
}
