/** 自定义组件的公开客户端接口；不暴露 Cookie、宿主路径或数据库连接。 */
import { useCallback } from 'react';
import { api } from './infrastructure/api';
import { useRuntime } from './presentation/runtime/context';
import type { JsonValue } from './domain/types';

export { ApiError } from './application/errors';
export { fileUrl } from './infrastructure/api';
export type { JsonValue, ApplicationDescription, SessionView } from './domain/types';
export { useField, useForm, useResource } from './presentation/runtime/bindings';
export type { FieldBinding, FormBinding, TableBinding, GroupBinding, PageBinding, ImageBinding } from './presentation/runtime/bindings';
export type { FieldDefinition, ResourceDefinition, SelectOption, OptionsSource, OptionsQuery, OptionsResult, JsonScalar, FieldValue, FileConstraints, UploadedFile, RecordData } from './domain/types';
export { Image, AttachmentList } from './presentation/images';
export type { ImageProps } from './presentation/images';
export type { FileUploadBinding, FileUploadState } from './application/uploads';
export { normalizeInput, validateField, validateRecord } from './domain/validation';
export { useI18n } from './presentation/i18n';
export type { I18nContextValue } from './presentation/i18n';
export type { I18n, I18nBundle, I18nFormatters, LanguageOption, MessageParams, Translate } from './domain/i18n/model';

export function callAction(name: string, args: JsonValue[] = []): Promise<JsonValue> {
  return api.executeAction(name, args);
}

export function useAction(name: string): (args?: JsonValue[]) => Promise<JsonValue> {
  const { client } = useRuntime();
  return useCallback((args: JsonValue[] = []) => client.executeAction(name, args), [client, name]);
}
