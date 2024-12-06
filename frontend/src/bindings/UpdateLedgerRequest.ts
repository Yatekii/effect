// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { BankFormat } from "./BankFormat";
import type { Currency } from "./Currency";

export type UpdateLedgerRequest = { format: BankFormat, initialBalance: number | null, initialDate: number, name: string, currency: Currency, spending: boolean, };