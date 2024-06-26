// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { Format } from "./Format";

export type Ledger = { id: string, name: string, currency: string, format: Format, transactions: {
        columns: { values: number[] }[];
    }, initial_balance: number | null, initial_date: number, spending: boolean, };