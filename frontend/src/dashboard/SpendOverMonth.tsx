import { BarChart, Title } from "@tremor/react";
import { SpendPerMonth } from "../bindings/SpendPerMonth";
import { valueFormatter } from "../lib/numbers";

export function SpendOverMonth({
  spendPerMonth,
}: {
  spendPerMonth: SpendPerMonth;
}) {
  const currentYear = new Date().getFullYear();
  const data = Object.entries(spendPerMonth.months).map(([month, year]) => {
    const o = {
      date: month,
    } as { [x: number]: number | null };

    for (const entry of Object.entries(year)) {
      o[+entry[0]] = Object.values(entry[1]).reduce((a, b) => a + b, 0);
    }

    for (let i = currentYear - 2; i <= currentYear; i++) {
      if (!o[i]) {
        o[i] = null;
      }
    }

    return o;
  });

  return (
    <>
      <Title>Spend per month</Title>
      <BarChart
        className="h-72 mt-4"
        data={data}
        index="date"
        categories={["2022", "2023", "2024"]}
        colors={["gray", "blue", "purple"]}
        yAxisWidth={50}
        valueFormatter={valueFormatter}
      />
    </>
  );
}
