// Test adapter uses the actual upstream LVX writer and BasePointsHandle.
#include "lvx_file.h"
#include <cassert>
#include <cstddef>
#include <cstring>
int main() {
  static_assert(sizeof(LvxFilePublicHeader)==24,"public");
  static_assert(sizeof(LvxFilePrivateHeader)==5,"private");
  static_assert(sizeof(LvxDeviceInfo)==59,"device");
  static_assert(sizeof(FrameHeader)==24,"frame");
  static_assert(offsetof(LvxBasePackDetail,raw_point)==19,"record");
  LvxFileHandle writer;assert(writer.InitLvxFile());
  LvxDeviceInfo device={};std::memcpy(device.lidar_broadcast_code,"reference",9);device.device_index=7;device.device_type=3;device.extrinsic_enable=1;device.roll=1.5f;device.x=-2.25f;
  writer.AddDeviceInfo(device);writer.InitLvxFileHeader();
  std::list<LvxBasePackDetail> packets;
  for (uint8_t kind=0;kind<=8;kind++) {
    uint8_t buffer[1600]={};auto* eth=reinterpret_cast<LivoxEthPacket*>(buffer);
    eth->version=5;eth->slot=1;eth->id=1;eth->timestamp_type=1;eth->data_type=kind;eth->err_code=0x12345678;
    uint64_t t=1000000000ULL+kind;std::memcpy(eth->timestamp,&t,8);
    for(size_t i=0;i<1400;i++)eth->data[i]=static_cast<uint8_t>(i);
    if(kind==6) {LivoxImuPoint p={1.25f,-2.5f,3.75f,0.5f,-0.25f,1.f};std::memcpy(eth->data,&p,sizeof(p));}
    LvxBasePackDetail packet={};packet.device_index=7;writer.BasePointsHandle(eth,packet);packets.push_back(packet);
  }
  writer.SaveFrameToLvxFile(packets);packets.clear();writer.SaveFrameToLvxFile(packets);writer.CloseLvxFile();
}
