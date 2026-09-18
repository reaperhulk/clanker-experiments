# SPDX-License-Identifier: LGPL-3.0-or-later
"""Small independent BMFF serializer for explicit test item graphs."""
import struct
from test_context import box,full


def item_file(items,primary=1,miaf=False):
    props=[];prop_ids={};entries=[];assoc=bytearray();locations=bytearray();refs=bytearray();data=bytearray()
    for item in items:
        ident,kind=item['id'],item['kind']
        infe=full(b'infe',struct.pack('>HH4s',ident,0,kind)+b'image\0',2)
        if ident!=primary:infe=infe[:11]+b'\1'+infe[12:]
        entries.append(infe)
        payload=item.get('data',b'')
        locations+=struct.pack('>HHHHII',ident,1,0,1,len(data),len(payload));data+=payload
        indices=[]
        for prop in item.get('props',[]):
            if prop not in prop_ids:props.append(prop);prop_ids[prop]=len(props)
            indices.append(prop_ids[prop])
        assoc+=struct.pack('>HB',ident,len(indices))+b''.join(struct.pack('>H',i) for i in indices)
        for relation,targets in item.get('refs',{}).items():
            refs+=box(relation,struct.pack('>HH',ident,len(targets))+b''.join(struct.pack('>H',i) for i in targets))
    ipma=full(b'ipma',struct.pack('>I',len(items))+assoc);ipma=ipma[:11]+b'\1'+ipma[12:]
    hdlr=full(b'hdlr',bytes(4)+b'pict'+bytes(12)+b'\0')
    meta=hdlr+full(b'pitm',struct.pack('>H',primary))+full(b'iinf',struct.pack('>H',len(items))+b''.join(entries))
    meta+=full(b'iloc',struct.pack('>BBH',68,0,len(items))+locations,1)+box(b'iprp',box(b'ipco',b''.join(props))+ipma)
    meta+=full(b'iref',refs)+box(b'idat',data)
    return box(b'ftyp',b'heic'+bytes(4)+b'mif1heic'+(b'miaf' if miaf else b''))+full(b'meta',meta)+box(b'free',bytes(32))


def ispe(width,height):return full(b'ispe',struct.pack('>II',width,height))
