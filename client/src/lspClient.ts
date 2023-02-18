import { ChildProcess, spawn } from 'child_process'
import { workspace } from 'vscode'
import * as lsp from 'vscode-languageclient/node'
import { LogMessageNotification } from 'vscode-languageclient/node'
import { Extension } from './extension'
import { log } from './log'
import * as constant from './constant'

export class LanguageClient extends lsp.LanguageClient {
  private extension: Extension

  constructor(ext: Extension, lspBinary: string) {
    const serverOptions = () => new Promise<ChildProcess>((resolve, reject) => {
      const childProcess = spawn(lspBinary, {
        env: {
          'RUST_BACKTRACE': 'true',
          ...process.env,
        }
      })
      resolve(childProcess)
    })

    super('mcshader', 'Minecraft Shaders LSP - Server', serverOptions, {
      diagnosticCollectionName: 'mcshader',
      documentSelector: [{ scheme: 'file', language: 'glsl' }],
      synchronize: {
        configurationSection: 'mcshader'
      },
    })
    this.extension = ext

    log.info('running with binary at path:\n\t', lspBinary)
  }

  public startServer = async (): Promise<LanguageClient> => {
    this.extension.context.subscriptions.push(this.onNotification(constant.statusNotificationMethod, this.onStatusChange))
    this.extension.context.subscriptions.push(this.onNotification(LogMessageNotification.method, this.logOutput))

    await this.start()

    return this
  }

  logOutput = (params: {
    type: 1 | 2 | 3 | 4
    message: string
  }) => {
    switch (params.type) {
      case 1:
        log.error(params.message)
        break
      case 2:
        log.warn(params.message)
        break
      case 3:
        log.info(params.message)
        break
      default:
        log.debug(params.message)
        break
    }
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
        this.extension.updateStatus(params.icon, params.message)
        break
      case 'clear':
        this.extension.clearStatus()
        break
    }
  }
}
