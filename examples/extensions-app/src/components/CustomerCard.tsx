import { useState } from 'react';
import { Alert, Button, Card, CardContent, Stack, Typography } from '@mui/material';
import clsx from 'clsx';
import { ApiError, useAction } from '@kenvoy/client';

interface CustomerCardProps {
  title: string;
  onIncrement: (count: number) => number;
}

export function CustomerCard({ title, onIncrement }: CustomerCardProps) {
  const [count, setCount] = useState(0);
  const [discount, setDiscount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState('');
  const calculate = useAction('calculateDiscount');

  async function calculateOnce() {
    if (busy) return;
    setBusy(true);
    setFailure('');
    try {
      const result = await calculate([10000]);
      if (typeof result !== 'number') throw new Error('服务端返回值无效');
      setDiscount(result);
    } catch (error) {
      setFailure(error instanceof ApiError && error.outcomeUnknown
        ? '请求结果待核对，系统没有自动重试。'
        : error instanceof Error ? error.message : '计算未完成');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className={clsx('customer-card-example', { active: count > 0 })}>
      <CardContent>
        <Stack spacing={2}>
          <Typography component="h2" variant="h5">{title}</Typography>
          <Typography>数量：{count}</Typography>
          <Stack direction="row" spacing={1} sx={{ flexWrap: 'wrap', gap: 1 }}>
            <Button variant="outlined" onClick={() => setCount(onIncrement(count))}>增加数量</Button>
            <Button variant="contained" disabled={busy} onClick={() => { void calculateOnce(); }}>
              {busy ? '正在计算…' : '计算折扣'}
            </Button>
          </Stack>
          {discount !== null && <Typography>折扣：{discount}</Typography>}
          {failure && <Alert severity="error">{failure}</Alert>}
        </Stack>
      </CardContent>
    </Card>
  );
}
