import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { Select, Toggle } from './components'

describe('Toggle', () => {
  it('reflects checked state and toggles on click', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<Toggle checked={false} onChange={onChange} />)
    const toggle = screen.getByRole('switch')
    expect(toggle).toHaveAttribute('aria-checked', 'false')
    await user.click(toggle)
    expect(onChange).toHaveBeenCalledWith(true)
  })
})

describe('Select', () => {
  it('opens menu and selects an option', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(
      <Select
        value="a"
        onChange={onChange}
        options={[
          { value: 'a', label: 'Option A' },
          { value: 'b', label: 'Option B' },
        ]}
      />,
    )
    expect(screen.getByRole('button', { name: /Option A/i })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Option A/i }))
    await user.click(screen.getByRole('option', { name: 'Option B' }))
    expect(onChange).toHaveBeenCalledWith('b')
  })
})
