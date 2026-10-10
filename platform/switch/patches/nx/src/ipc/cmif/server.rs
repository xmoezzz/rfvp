use super::*;
use core::mem as cmem;

#[inline(always)]
pub fn read_command_from_msg_buffer(ctx: &mut CommandContext) -> CommandType {
    unsafe {
        let mut ipc_buf = get_msg_buffer();

        let command_header = ipc_buf as *mut CommandHeader;
        ipc_buf = command_header.offset(1) as *mut u8;

        let command_type = (*command_header).get_command_type();
        let data_size = (*command_header).get_data_word_count() * cmem::size_of::<u32>() as u32;
        ctx.in_params.data_size = data_size;

        if (*command_header).get_has_special_header() {
            let special_header = ipc_buf as *mut CommandSpecialHeader;
            ipc_buf = special_header.offset(1) as *mut u8;

            ctx.in_params.send_process_id = (*special_header).get_send_process_id();
            if ctx.in_params.send_process_id {
                let process_id_ptr = ipc_buf as *mut u64;
                ctx.in_params.process_id = *process_id_ptr;
                ipc_buf = process_id_ptr.offset(1) as *mut u8;
            }

            let copy_handle_count = (*special_header).get_copy_handle_count();
            ipc_buf =
                read_array_from_buffer(ipc_buf, copy_handle_count, &mut ctx.in_params.copy_handles);
            let move_handle_count = (*special_header).get_move_handle_count();
            ipc_buf =
                read_array_from_buffer(ipc_buf, move_handle_count, &mut ctx.in_params.move_handles);
        }

        let send_static_count = (*command_header).get_send_static_count();
        ipc_buf = read_array_from_buffer(ipc_buf, send_static_count, &mut ctx.send_statics);
        let send_buffer_count = (*command_header).get_send_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, send_buffer_count, &mut ctx.send_buffers);
        let receive_buffer_count = (*command_header).get_receive_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, receive_buffer_count, &mut ctx.receive_buffers);
        let exchange_buffer_count = (*command_header).get_exchange_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, exchange_buffer_count, &mut ctx.exchange_buffers);

        ctx.in_params.data_words_offset = ipc_buf;
        ipc_buf = ipc_buf.offset(data_size as isize);

        let receive_static_count = (*command_header).get_receive_static_count();
        read_array_from_buffer(ipc_buf, receive_static_count, &mut ctx.receive_statics);

        convert_command_type(command_type)
    }
}

#[inline(always)]
pub fn write_command_response_on_msg_buffer(
    ctx: &mut CommandContext,
    command_type: CommandType,
    data_size: u32,
) {
    unsafe {
        let mut ipc_buf = get_msg_buffer();

        let command_header = ipc_buf as *mut CommandHeader;
        ipc_buf = command_header.offset(1) as *mut u8;

        let data_word_count = data_size.div_ceil(4);
        let has_special_header = ctx.out_params.send_process_id
            || !ctx.out_params.copy_handles.is_empty()
            || !ctx.out_params.move_handles.is_empty();
        *command_header = CommandHeader::new(
            command_type as u32,
            ctx.send_statics.len() as u32,
            ctx.send_buffers.len() as u32,
            ctx.receive_buffers.len() as u32,
            ctx.exchange_buffers.len() as u32,
            data_word_count,
            ctx.receive_statics.len() as u32,
            has_special_header,
        );

        if has_special_header {
            let special_header = ipc_buf as *mut CommandSpecialHeader;
            ipc_buf = special_header.offset(1) as *mut u8;

            *special_header = CommandSpecialHeader::new(
                ctx.out_params.send_process_id,
                ctx.out_params.copy_handles.len() as u32,
                ctx.out_params.move_handles.len() as u32,
            );
            if ctx.out_params.send_process_id {
                ipc_buf = ipc_buf.add(cmem::size_of::<u64>());
            }

            ipc_buf = write_array_to_buffer(
                ipc_buf,
                ctx.out_params.copy_handles.len() as u32,
                &ctx.out_params.copy_handles,
            );
            ipc_buf = write_array_to_buffer(
                ipc_buf,
                ctx.out_params.move_handles.len() as u32,
                &ctx.out_params.move_handles,
            );
        }

        ipc_buf = write_array_to_buffer(ipc_buf, ctx.send_statics.len() as u32, &ctx.send_statics);
        ipc_buf = write_array_to_buffer(ipc_buf, ctx.send_buffers.len() as u32, &ctx.send_buffers);
        ipc_buf = write_array_to_buffer(
            ipc_buf,
            ctx.receive_buffers.len() as u32,
            &ctx.receive_buffers,
        );
        ipc_buf = write_array_to_buffer(
            ipc_buf,
            ctx.exchange_buffers.len() as u32,
            &ctx.exchange_buffers,
        );
        ctx.out_params.data_words_offset = ipc_buf;

        ipc_buf = ipc_buf.offset((data_word_count * cmem::size_of::<u32>() as u32) as isize);
        write_array_to_buffer(
            ipc_buf,
            ctx.receive_statics.len() as u32,
            &ctx.receive_statics,
        );
    }
}

#[inline(always)]
pub fn read_request_command_from_msg_buffer(
    ctx: &mut CommandContext,
) -> Result<(u32, DomainCommandType, DomainObjectId)> {
    unsafe {
        let mut domain_command_type = DomainCommandType::Invalid;
        let mut domain_object_id: DomainObjectId = 0;
        let ipc_buf = get_msg_buffer();
        let mut data_ptr = get_aligned_data_offset(ctx.in_params.data_words_offset, ipc_buf);

        let mut data_header = data_ptr as *mut DataHeader;
        if ctx.object_info.is_domain() {
            let domain_header = data_ptr as *mut DomainInDataHeader;
            data_ptr = domain_header.offset(1) as *mut u8;
            ctx.in_params.data_size -= cmem::size_of::<DomainInDataHeader>() as u32;

            domain_command_type = (*domain_header).command_type;
            let object_count = (*domain_header).object_count;
            domain_object_id = (*domain_header).domain_object_id;
            let objects_offset = data_ptr.offset((*domain_header).data_size as isize);
            read_array_from_buffer(
                objects_offset,
                object_count as u32,
                &mut ctx.in_params.objects,
            );

            data_header = data_ptr as *mut DataHeader;
        }

        let mut rq_id: u32 = 0;
        if ctx.in_params.data_size >= DATA_PADDING {
            ctx.in_params.data_size -= DATA_PADDING;
            if ctx.in_params.data_size >= cmem::size_of::<DataHeader>() as u32 {
                result_return_unless!(
                    (*data_header).magic == IN_DATA_HEADER_MAGIC,
                    super::rc::ResultInvalidInputHeader
                );

                rq_id = (*data_header).value;
                data_ptr = data_header.offset(1) as *mut u8;
                ctx.in_params.data_size -= cmem::size_of::<DataHeader>() as u32;
            }
        }

        ctx.in_params.data_offset = data_ptr;
        Ok((rq_id, domain_command_type, domain_object_id))
    }
}

#[inline(always)]
pub fn write_request_command_response_on_msg_buffer(
    ctx: &mut CommandContext,
    result: ResultCode,
    request_type: CommandType,
) {
    unsafe {
        let ipc_buf = get_msg_buffer();
        let mut data_size =
            DATA_PADDING + cmem::size_of::<DataHeader>() as u32 + ctx.out_params.data_size;
        if ctx.object_info.is_domain() {
            data_size += (cmem::size_of::<DomainOutDataHeader>()
                + cmem::size_of::<DomainObjectId>() * ctx.out_params.objects.len())
                as u32;
        }
        data_size = (data_size + 1) & !1;

        write_command_response_on_msg_buffer(ctx, request_type, data_size);
        let mut data_offset = get_aligned_data_offset(ctx.out_params.data_words_offset, ipc_buf);

        let mut data_header = data_offset as *mut DataHeader;
        if ctx.object_info.is_domain() {
            let domain_header = data_offset as *mut DomainOutDataHeader;
            data_offset = domain_header.offset(1) as *mut u8;
            *domain_header = DomainOutDataHeader::new(ctx.out_params.objects.len() as u32);
            let objects_offset =
                data_offset.add(cmem::size_of::<DataHeader>() + ctx.out_params.data_size as usize);
            write_array_to_buffer(
                objects_offset,
                ctx.out_params.objects.len() as u32,
                &ctx.out_params.objects,
            );
            data_header = data_offset as *mut DataHeader;
        }
        data_offset = data_header.offset(1) as *mut u8;

        let version: u32 = match request_type {
            CommandType::RequestWithContext => 1,
            _ => 0,
        };
        *data_header = DataHeader::new(OUT_DATA_HEADER_MAGIC, version, result.get_value(), 0);
        ctx.out_params.data_offset = data_offset;
    }
}

#[inline(always)]
pub fn read_control_command_from_msg_buffer(ctx: &mut CommandContext) -> Result<ControlRequestId> {
    unsafe {
        let ipc_buf = get_msg_buffer();
        let mut data_offset = get_aligned_data_offset(ctx.in_params.data_words_offset, ipc_buf);

        let data_header = data_offset as *mut DataHeader;
        data_offset = data_header.offset(1) as *mut u8;

        result_return_unless!(
            (*data_header).magic == IN_DATA_HEADER_MAGIC,
            super::rc::ResultInvalidInputHeader
        );
        let control_rq_id = (*data_header).value;

        ctx.in_params.data_offset = data_offset;
        ctx.in_params.data_size -= DATA_PADDING + cmem::size_of::<DataHeader>() as u32;
        Ok(cmem::transmute::<u32, ControlRequestId>(control_rq_id))
    }
}

#[inline(always)]
pub fn write_control_command_response_on_msg_buffer(
    ctx: &mut CommandContext,
    result: ResultCode,
    control_type: CommandType,
) {
    unsafe {
        let ipc_buf = get_msg_buffer();
        let mut data_size =
            DATA_PADDING + cmem::size_of::<DataHeader>() as u32 + ctx.out_params.data_size;
        data_size = (data_size + 1) & !1;

        write_command_response_on_msg_buffer(ctx, control_type, data_size);
        let mut data_offset = get_aligned_data_offset(ctx.out_params.data_words_offset, ipc_buf);

        let data_header = data_offset as *mut DataHeader;
        data_offset = data_header.offset(1) as *mut u8;

        let version: u32 = match control_type {
            CommandType::ControlWithContext => 1,
            _ => 0,
        };
        *data_header = DataHeader::new(OUT_DATA_HEADER_MAGIC, version, result.get_value(), 0);
        ctx.out_params.data_offset = data_offset;
    }
}

#[inline(always)]
pub fn write_close_command_response_on_msg_buffer(ctx: &mut CommandContext) {
    write_command_response_on_msg_buffer(ctx, CommandType::Close, 0);
}
