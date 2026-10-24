/**
 * Game exit-code classification (HMCL `ExitWaiter` + POSIX signal semantics).
 *
 * HMCL treats exit 0 as NORMAL; non-zero becomes APPLICATION_ERROR, with 137
 * on Linux/BSD specially marked SIGKILL. We extend that with the standard
 * 128+signal convention used by unix process exit statuses.
 */
export interface ExitDescription {
  severity: "normal" | "error" | "fatal";
  /** i18n key under `logs.exit.*` */
  key: string;
  params?: Record<string, string | number>;
}

export function describeExit(exitCode: number): ExitDescription {
  if (exitCode === 0) return { severity: "normal", key: "logs.exit.normal" };
  switch (exitCode) {
    case 1:
      return { severity: "error", key: "logs.exit.appError", params: { code: 1 } };
    case 134: // SIGABRT
      return { severity: "fatal", key: "logs.exit.sigabrt", params: { code: 134 } };
    case 137: // SIGKILL（常见：系统 OOM 或被强制结束）
      return { severity: "fatal", key: "logs.exit.sigkill", params: { code: 137 } };
    case 139: // SIGSEGV
      return { severity: "fatal", key: "logs.exit.sigsegv", params: { code: 139 } };
    case 143: // SIGTERM
      return { severity: "error", key: "logs.exit.sigterm", params: { code: 143 } };
    default:
      break;
  }
  if (exitCode > 128 && exitCode < 160) {
    return { severity: "fatal", key: "logs.exit.signal", params: { code: exitCode, signal: exitCode - 128 } };
  }
  return { severity: "error", key: "logs.exit.abnormal", params: { code: exitCode } };
}
