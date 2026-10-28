// Read the two named bitmap resources in the owner's PE32 executable. No code is executed.
export function extractBranding(exe) {
  const view=new DataView(exe.buffer,exe.byteOffset,exe.byteLength);
  const check=(p,n)=>{if(!Number.isSafeInteger(p)||p<0||n<0||p+n>exe.length)throw new Error('Invalid bitmap resource');return p;};
  const u16=p=>view.getUint16(check(p,2),true),u32=p=>view.getUint32(check(p,4),true);
  if(u16(0)!==0x5a4d)throw new Error('Invalid PE signature');
  const pe=u32(0x3c),opt=pe+24;
  if(u32(pe)!==0x4550||u16(opt)!==0x10b)throw new Error('Expected PE32');
  const count=u16(pe+6),sections=opt+u16(pe+20);
  if(count<1||count>96)throw new Error('Invalid PE sections');
  function rva(address,length){
    for(let i=0;i<count;i++){
      const p=sections+i*40,base=u32(p+12),size=u32(p+16);
      if(address>=base&&address-base+length<=size)return check(u32(p+20)+address-base,length);
    }
    throw new Error('Invalid resource address');
  }
  const resourceSize=u32(opt+116),root=rva(u32(opt+112),resourceSize);
  const at=(offset,n)=>{if(offset<0||offset+n>resourceSize)throw new Error('Invalid resource directory');return check(root+offset,n);};
  function entries(offset){
    const p=at(offset,16),count=u16(p+12)+u16(p+14);
    if(count>4096)throw new Error('Invalid resource count');
    return Array.from({length:count},(_,i)=>{const e=at(offset+16+i*8,8);return {name:u32(e),value:u32(e+4)}});
  }
  const bitmap=entries(0).find(e=>e.name===2);
  if(!bitmap||(bitmap.value>>>31)!==1)throw new Error('Missing bitmap resources');
  const names=new Map([['FORM1_WSR_LOGO_BMP','WSR-BANNER.BMP'],['FORM1_FINANHEADLINE_BMP','WSR-NEWS.BMP']]);
  const result=[];
  for(const entry of entries(bitmap.value&0x7fffffff)){
    if(!(entry.name>>>31))continue;
    const pos=entry.name&0x7fffffff,len=u16(at(pos,2));
    if(len>256)throw new Error('Invalid resource name');
    const name=new TextDecoder('utf-16le').decode(exe.subarray(at(pos+2,len*2),root+pos+2+len*2));
    if(!names.has(name))continue;
    if(!(entry.value>>>31))throw new Error('Invalid bitmap directory');
    const language=entries(entry.value&0x7fffffff)[0];
    if(!language||(language.value>>>31))throw new Error('Missing bitmap data');
    const data=at(language.value,16),size=u32(data+4),p=rva(u32(data),size),header=u32(p);
    if(size<40)throw new Error('Truncated bitmap header');
    check(p,40);
    const width=view.getInt32(p+4,true),height=view.getInt32(p+8,true);
    if(header<40||header>size||width<1||width>4096||height<1||height>1024||u16(p+12)!==1||u16(p+14)!==24||u32(p+16)!==0)throw new Error('Unsupported bitmap format');
    if(header+Math.ceil(width*3/4)*4*height>size)throw new Error('Truncated bitmap');
    const bytes=new Uint8Array(14+size),out=new DataView(bytes.buffer);
    bytes.set([0x42,0x4d]);out.setUint32(2,bytes.length,true);out.setUint32(10,14+header,true);bytes.set(exe.subarray(p,p+size),14);
    result.push({name:names.get(name),bytes});
  }
  if(result.length!==2)throw new Error('Missing Wall Street Raider artwork');
  return result;
}
