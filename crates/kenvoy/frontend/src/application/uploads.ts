import { ApiError } from './errors.ts';
import type { Client } from './ports.ts';
import { acceptsFile, formatFileSize, isFileConstraints, isUploadedFiles, validFileName, validMime } from '../domain/files.ts';
import type { FieldDefinition, UploadedFile } from '../domain/types.ts';
import type { LocalizedMessage } from '../domain/i18n/error.ts';

export interface FileUploadState {
  busy: boolean;
  canceled: boolean;
  completed: number;
  total: number;
  removing: readonly string[];
  error?: string;
  error_message?: LocalizedMessage;
  outcomeUnknown: boolean;
}
export interface FileUploadBinding {
  uploadState: Readonly<FileUploadState>;
  upload: (files: Iterable<File>) => Promise<void>;
  removeUpload: (file: UploadedFile | string) => Promise<void>;
  cancelUpload: () => void;
}
export const EMPTY_UPLOAD_STATE: Readonly<FileUploadState> = Object.freeze({ busy: false, canceled: false, completed: 0, total: 0, removing: Object.freeze([]), outcomeUnknown: false });
type UploadTransport = Pick<Client, 'upload' | 'removeUpload'>;
type UploadOperation = { epoch: number; canceled: boolean };
interface UploadScope {
  resource: string;
  formId?: string;
  mode: 'create' | 'update';
  read: (name: string) => unknown;
  accept: (name: string, files: UploadedFile[] | null) => void;
  changed: () => void;
  editable: () => boolean;
}

/** 表单独占暂存文件；取消与迟到响应不能回填其他表单，提交中的附件不作卸载清理。 */
export class FormUploads {
  #client: UploadTransport;
  #scope: UploadScope;
  #epoch = 0;
  #live = true;
  #preserve = false;
  #files = new Map<string, UploadedFile[]>();
  #staged = new Map<string, { field: string; file: UploadedFile }>();
  #states = new Map<string, FileUploadState>();
  #active = new Map<string, UploadOperation>();
  #cleanupStarted = new Set<string>();
  constructor(client: UploadTransport, scope: UploadScope) { this.#client = client; this.#scope = scope; }
  activate(): void { this.#live = true; }
  state(name: string): Readonly<FileUploadState> { return this.#states.get(name) ?? EMPTY_UPLOAD_STATE; }
  get blocked(): boolean { return this.#active.size > 0 || [...this.#states.values()].some(state => state.removing.length > 0 || state.outcomeUnknown); }
  get hasWork(): boolean { return this.#staged.size > 0 || this.blocked || [...this.#states.values()].some(state => Boolean(state.error)); }
  #current(name: string): UploadedFile[] {
    if (!this.#files.has(name)) {
      const value = this.#scope.read(name);
      this.#files.set(name, isUploadedFiles(value) ? [...value] : []);
    }
    return this.#files.get(name)!;
  }
  #setFiles(name: string, files: UploadedFile[] | null): void {
    this.#files.set(name, files ?? []);
    if (this.#live) this.#scope.accept(name, files);
  }
  #state(name: string, patch: Partial<FileUploadState>): void {
    if (Object.hasOwn(patch, 'error') && patch.error === undefined) patch.error_message = undefined;
    this.#states.set(name, { ...EMPTY_UPLOAD_STATE, ...this.#states.get(name), ...patch });
    if (this.#live) this.#scope.changed();
  }
  #failure(name: string, error: unknown): void {
    const message: LocalizedMessage = error instanceof ApiError
      ? { message: error.message, message_key: error.message_key ?? error.code, params: error.params, outcomeUnknown: error.outcomeUnknown }
      : { message: error instanceof Error ? error.message : '', message_key: error instanceof Error ? undefined : 'ui.files.failed' };
    this.#state(name, { error: error instanceof Error ? error.message : '文件操作未完成，请重新选择文件或取消本次填写。',
      error_message: message, outcomeUnknown: error instanceof ApiError && error.outcomeUnknown });
  }
  #available(name: string): void {
    if (!this.#live || this.#preserve || !this.#scope.editable() || this.state(name).outcomeUnknown) throw new ApiError('当前表单不能修改附件，请先完成或取消本次填写', { message_key: 'ui.files.unavailable' });
  }
  async upload(field: FieldDefinition, selected: Iterable<File>): Promise<void> {
    this.#available(field.name);
    if (this.#active.has(field.name) || this.state(field.name).removing.length) throw new ApiError('此字段的文件操作正在进行', { message_key: 'ui.files.pending' });
    const files = [...selected];
    if (files.length === 0) return;
    let invalid: ApiError | undefined;
    if (field.kind !== 'file' || !isFileConstraints(field.upload)) invalid = new ApiError('此字段未配置文件上传', { message_key: 'ui.files.not_configured' });
    else if (this.#current(field.name).length + files.length > field.upload.max_files) invalid = new ApiError(`最多可上传 ${field.upload.max_files} 个文件，请减少选择数量`,
      { message_key: 'ui.files.too_many', params: { max: field.upload.max_files } });
    else for (const file of files) {
      if (!(file instanceof File) || !validFileName(file.name)) { invalid = new ApiError('文件名无效，不能包含路径、控制字符或超过 255 个字符', { message_key: 'ui.files.invalid_name' }); break; }
      if (file.size < 1 || file.size > field.upload.max_size) {
        invalid = new ApiError(`${file.name} 的大小不符合要求，单文件应为 1 B 至 ${formatFileSize(field.upload.max_size)}`,
          { message_key: 'ui.files.invalid_size', params: { name: file.name, size: formatFileSize(field.upload.max_size) } }); break;
      }
      if (!validMime(file.type || 'application/octet-stream') || !acceptsFile(file.type || 'application/octet-stream', field.upload.accept)) {
        invalid = new ApiError(`${file.name} 的类型不符合要求，允许：${field.upload.accept.join('、') || '规范 MIME 类型'}`,
          { message_key: 'ui.files.invalid_type', params: { name: file.name, types: field.upload.accept.join(', ') || 'MIME' } }); break;
      }
    }
    if (invalid) { this.#failure(field.name, invalid); throw invalid; }
    const operation = { epoch: this.#epoch, canceled: false };
    this.#active.set(field.name, operation);
    this.#state(field.name, { busy: true, canceled: false, completed: 0, total: files.length, error: undefined, outcomeUnknown: false });
    try {
      for (const file of files) {
        if (!this.#isCurrent(field.name, operation)) break;
        const uploaded = await this.#client.upload(this.#scope.resource, field.name, file, this.#scope.formId, this.#scope.mode);
        if (!this.#isCurrent(field.name, operation)) { await this.#discard(field.name, uploaded, -1); break; }
        this.#staged.set(uploaded.id, { field: field.name, file: uploaded });
        this.#setFiles(field.name, [...this.#current(field.name), uploaded]);
        this.#state(field.name, { completed: this.state(field.name).completed + 1 });
      }
    } catch (error) {
      if (this.#isCurrent(field.name, operation)) { this.#failure(field.name, error); throw error; }
    } finally {
      if (this.#isCurrent(field.name, operation)) { this.#active.delete(field.name); this.#state(field.name, { busy: false }); }
    }
  }
  #isCurrent(name: string, operation: UploadOperation): boolean {
    return this.#live && !operation.canceled && operation.epoch === this.#epoch && this.#active.get(name) === operation;
  }
  cancel(name: string): void {
    const operation = this.#active.get(name);
    if (!operation) return;
    operation.canceled = true;
    this.#active.delete(name);
    this.#state(name, { busy: false, canceled: true, error: undefined });
  }
  async remove(name: string, file: UploadedFile | string): Promise<void> {
    this.#available(name);
    const id = typeof file === 'string' ? file : file.id;
    if (this.#active.has(name)) throw new ApiError('请先等待或取消此字段的上传', { message_key: 'ui.files.wait_upload' });
    if (this.state(name).removing.includes(id) || !this.#current(name).some(item => item.id === id)) return;
    const staged = this.#staged.get(id);
    if (!staged) { this.#setFiles(name, this.#current(name).filter(item => item.id !== id)); return; }
    const epoch = this.#epoch;
    this.#state(name, { removing: [...this.state(name).removing, id], error: undefined });
    this.#cleanupStarted.add(id);
    try {
      await this.#client.removeUpload(id);
      this.#staged.delete(id);
      if (this.#live && epoch === this.#epoch) this.#setFiles(name, this.#current(name).filter(item => item.id !== id));
    } catch (error) {
      if (this.#live && epoch === this.#epoch) this.#failure(name, error);
      throw error;
    } finally {
      if (this.#live && epoch === this.#epoch) this.#state(name, { removing: this.state(name).removing.filter(value => value !== id) });
    }
  }
  async replace(name: string, files: UploadedFile[] | null): Promise<void> {
    this.#available(name);
    if (this.#active.has(name) || this.state(name).removing.length) throw new ApiError('请先等待或取消此字段的文件操作', { message_key: 'ui.files.wait_operation' });
    const epoch = this.#epoch;
    for (const previous of [...this.#current(name)]) if (!files?.some(file => file.id === previous.id)) await this.remove(name, previous.id);
    if (this.#live && epoch === this.#epoch) this.#setFiles(name, files === null ? null : [...files]);
  }
  retainForSubmission(): void { this.#preserve = true; }
  committed(): void { this.#preserve = true; this.#staged.clear(); }
  submissionFailed(outcomeUnknown: boolean): void {
    this.#preserve = outcomeUnknown;
    if (!this.#live && !outcomeUnknown) this.#cleanup();
  }
  #cleanup(epoch = this.#epoch): void {
    for (const { field, file } of this.#staged.values()) void this.#discard(field, file, epoch);
    this.#staged.clear();
  }
  async #discard(name: string, file: UploadedFile, epoch: number): Promise<void> {
    if (this.#cleanupStarted.has(file.id)) return;
    this.#cleanupStarted.add(file.id);
    try { await this.#client.removeUpload(file.id); }
    catch (error) { if (this.#live && epoch === this.#epoch) this.#failure(name, error); }
  }
  reset(): void {
    this.dispose();
    this.#files.clear(); this.#states.clear(); this.#preserve = false; this.#live = true;
    this.#scope.changed();
  }
  dispose(): void {
    const previousEpoch = this.#epoch++;
    this.#live = false;
    for (const operation of this.#active.values()) operation.canceled = true;
    this.#active.clear();
    if (!this.#preserve) this.#cleanup(previousEpoch);
  }
}
