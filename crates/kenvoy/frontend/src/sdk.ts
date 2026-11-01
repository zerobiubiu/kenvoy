/** 自定义组件的公开客户端接口；不暴露 Cookie、宿主路径或数据库连接。 */
import { useCallback } from 'react';
import { api } from './api';
import { useRuntime } from './context';
import type { JsonValue } from './types';

export { ApiError, fileUrl } from './api';
export type { JsonValue, ApplicationDescription, SessionView } from './types';
export { useField, useForm, useResource } from './bindings';
export type { FieldBinding, FormBinding, TableBinding, GroupBinding, PageBinding, ImageBinding } from './bindings';
export type { FieldDefinition, ResourceDefinition, SelectOption, OptionsSource, OptionsQuery, OptionsResult, JsonScalar, FieldValue, FileConstraints, UploadedFile, RecordData } from './types';
export { Image, AttachmentList } from './images';
export type { ImageProps } from './images';
export type { FileUploadBinding, FileUploadState } from './uploads';
export { normalizeInput, validateField, validateRecord } from './validation';

export function callAction(name: string, args: JsonValue[] = []): Promise<JsonValue> {
  return api.executeAction(name, args);
}

export function useAction(name: string): (args?: JsonValue[]) => Promise<JsonValue> {
  const { client } = useRuntime();
  return useCallback((args: JsonValue[] = []) => client.executeAction(name, args), [client, name]);
}
