import type { ClientModule } from 'claude-code'

import type { LabelMessage } from '../types'
import { LABEL_GAP, labelIndexAt, labelText } from './labelLayout'

type LabelsProps = { numbers: number[] }
type LabelsState = { numbers: number[] }

const ImageLabels: ClientModule<LabelsProps, LabelsState> = ({ numbers }, surface) => {
  if (surface.state === undefined) {
    const state: LabelsState = { numbers }
    surface.setState(state)
    surface.onPointer(event => {
      if (event.type !== 'up' || event.button !== 'left') return
      const index = labelIndexAt(event.x, state.numbers)
      const number = index === undefined ? undefined : state.numbers[index]
      if (number !== undefined) surface.post({ number } satisfies LabelMessage)
    })
  } else {
    surface.state.numbers = numbers
  }

  const { Box, Text } = surface.elements
  return (
    <Box flexDirection="row">
      {numbers.map(number => (
        <Box key={`label-${number}`} marginRight={LABEL_GAP}>
          <Text color="cyan" hover={{ underline: true }}>
            {labelText(number)}
          </Text>
        </Box>
      ))}
    </Box>
  )
}

export default ImageLabels
