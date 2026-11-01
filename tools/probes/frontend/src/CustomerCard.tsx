import { useState } from 'react';
import { Button, Card, CardContent, Stack, Typography } from '@mui/material';

export function CustomerCard({ title }: { title: string }) {
  const [count, setCount] = useState(0);
  return (
    <Card className="probe-card" variant="outlined">
      <CardContent>
        <Stack spacing={2}>
          <Typography variant="h6">{title}</Typography>
          <span data-testid="refresh-label">组件修改前</span>
          <Button onClick={() => setCount((value) => value + 1)} data-testid="counter">
            点击次数：{count}
          </Button>
        </Stack>
      </CardContent>
    </Card>
  );
}
