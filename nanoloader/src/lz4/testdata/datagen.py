from typing import Optional

import lz4.block

lorem = '''Lorem ipsum dolor sit amet consectetur adipiscing elit. Quisque faucibus ex
sapien vitae pellentesque sem placerat. In id cursus mi pretium tellus duis
convallis. Tempus leo eu aenean sed diam urna tempor. Pulvinar vivamus
fringilla lacus nec metus bibendum egestas. Iaculis massa nisl malesuada
lacinia integer nunc posuere. Ut hendrerit semper vel class aptent taciti
sociosqu. Ad litora torquent per conubia nostra inceptos himenaeos.'''.encode('utf-8')

def create(name:str, data:bytes, dictionary:Optional[bytes]=None) -> None:
    with open(f'{name}.dat', 'wb') as fh:
        fh.write(data)
    if dictionary:
        with open(f'{name}.dct', 'wb') as fh:
            fh.write(dictionary)

    with open(f'{name}.lz4', 'wb') as fh:
        fh.write(lz4.block.compress(data, dict=dictionary, mode='high_compression', compression=12, store_size=False))


create('lorem1', lorem)
create('lorem2', lorem, 'Iaculis massa nisl malesuada'.encode('utf-8'))
