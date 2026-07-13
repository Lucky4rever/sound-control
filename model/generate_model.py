"""
Генерує CNN для класифікації: 0 = голос (speech), 1 = музика/аудіо (music).
Input:  [1, 1, 64, 100] — mel-spectrogram
Output: [1, 2] — logits

Запуск:
    pip install torch
    python model/generate_model.py
"""
import torch
import torch.nn as nn

class TinyAudioClassifier(nn.Module):
    def __init__(self):
        super().__init__()
        self.conv1 = nn.Conv2d(1, 8, kernel_size=(3, 3), padding=1)
        self.pool1 = nn.MaxPool2d((2, 2))
        self.conv2 = nn.Conv2d(8, 16, kernel_size=(3, 3), padding=1)
        self.pool2 = nn.MaxPool2d((2, 2))
        self.fc1 = nn.Linear(16 * 16 * 25, 32)
        self.fc2 = nn.Linear(32, 2)
        self.dropout = nn.Dropout(0.3)

    def forward(self, x):
        x = self.pool1(torch.relu(self.conv1(x)))
        x = self.pool2(torch.relu(self.conv2(x)))
        x = x.view(x.size(0), -1)
        x = torch.relu(self.fc1(x))
        x = self.dropout(x)
        x = self.fc2(x)
        return x

model = TinyAudioClassifier()
model.eval()

dummy_input = torch.randn(1, 1, 64, 100)
torch.onnx.export(
    model,
    dummy_input,
    "assets/model.onnx",
    input_names=["input"],
    output_names=["output"],
    dynamic_axes={"input": {0: "batch_size"}, "output": {0: "batch_size"}},
    opset_version=11,
)
print("✅ model.onnx збережено в assets/model.onnx")