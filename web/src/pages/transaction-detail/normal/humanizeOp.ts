import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';

import { formatOperationType } from '../../transactions/operationTypes.js';

function shortId(value: string): string {
  return value.length > 12 ? `${value.slice(0, 6)}…${value.slice(-4)}` : value;
}

function fnNameFromHeavy(heavy: XdrOperationDto | null): string | null {
  const details = heavy?.details;
  if (details && typeof details === 'object' && !Array.isArray(details)) {
    const fn = (details as { function_name?: unknown }).function_name;
    if (typeof fn === 'string' && fn.length > 0) return fn;
  }
  return null;
}

function summaryFromHeavy(heavy: XdrOperationDto | null): string | null {
  const details = heavy?.details;
  if (details && typeof details === 'object' && !Array.isArray(details)) {
    const value = (details as { summary?: unknown }).summary;
    if (typeof value === 'string' && value.length > 0) return value;
  }
  return null;
}

export function humanizeOp(
  light: OperationItem,
  heavy: XdrOperationDto | null
): string {
  const explicit = summaryFromHeavy(heavy);
  if (explicit != null) return explicit;

  const opLabel = formatOperationType(light.type_name);

  switch (light.type_name) {
    case 'PAYMENT':
    case 'PATH_PAYMENT_STRICT_RECEIVE':
    case 'PATH_PAYMENT_STRICT_SEND':
      if (light.destination_account != null) {
        const asset = light.asset_code ?? 'XLM';
        return `Sent ${asset} to ${shortId(light.destination_account)}`;
      }
      break;
    case 'INVOKE_HOST_FUNCTION': {
      const fn = fnNameFromHeavy(heavy);
      if (fn != null && light.contract_id != null) {
        return `Called ${fn}() on ${shortId(light.contract_id)}`;
      }
      if (fn != null) return `Called ${fn}()`;
      if (light.contract_id != null) {
        return `Invoked contract ${shortId(light.contract_id)}`;
      }
      break;
    }
    case 'CREATE_ACCOUNT':
      if (light.destination_account != null) {
        return `Created account ${shortId(light.destination_account)}`;
      }
      break;
  }

  return `${opLabel} processed`;
}
