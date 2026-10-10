use super::*;
use core::mem as cmem;

#[inline(always)]
pub fn write_command_on_msg_buffer(
    ctx: &mut CommandContext,
    command_type: CommandType,
    data_size: u32,
) {
    unsafe {
        let mut ipc_buf = get_msg_buffer();

        let has_special_header = ctx.in_params.send_process_id
            || !ctx.in_params.copy_handles.is_empty()
            || !ctx.in_params.move_handles.is_empty();
        let data_word_count = data_size.div_ceil(4);
        let command_header = ipc_buf as *mut CommandHeader;
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
        ipc_buf = command_header.offset(1) as *mut u8;

        if has_special_header {
            let special_header = ipc_buf as *mut CommandSpecialHeader;
            *special_header = CommandSpecialHeader::new(
                ctx.in_params.send_process_id,
                ctx.in_params.copy_handles.len() as u32,
                ctx.in_params.move_handles.len() as u32,
            );
            ipc_buf = special_header.offset(1) as *mut u8;

            if ctx.in_params.send_process_id {
                ipc_buf = ipc_buf.add(cmem::size_of::<u64>());
            }

            ipc_buf = write_array_to_buffer(
                ipc_buf,
                ctx.in_params.copy_handles.len() as u32,
                &ctx.in_params.copy_handles,
            );
            ipc_buf = write_array_to_buffer(
                ipc_buf,
                ctx.in_params.move_handles.len() as u32,
                &ctx.in_params.move_handles,
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
        ctx.in_params.data_words_offset = ipc_buf;
        ipc_buf = ipc_buf.add(cmem::size_of::<u32>() * data_word_count as usize);
        write_array_to_buffer(
            ipc_buf,
            ctx.receive_statics.len() as u32,
            &ctx.receive_statics,
        );
    }
}

#[inline(always)]
pub fn read_command_response_from_msg_buffer(ctx: &mut CommandContext) {
    unsafe {
        let mut ipc_buf = get_msg_buffer();

        let command_header = ipc_buf as *mut CommandHeader;
        ipc_buf = command_header.offset(1) as *mut u8;

        let mut copy_handle_count: u32 = 0;
        let mut move_handle_count: u32 = 0;
        if (*command_header).get_has_special_header() {
            let special_header = ipc_buf as *mut CommandSpecialHeader;
            copy_handle_count = (*special_header).get_copy_handle_count();
            move_handle_count = (*special_header).get_move_handle_count();
            ipc_buf = special_header.offset(1) as *mut u8;
            if (*special_header).get_send_process_id() {
                ctx.out_params.process_id = *(ipc_buf as *mut u64);
                ipc_buf = ipc_buf.add(cmem::size_of::<u64>());
            }
        }

        ipc_buf =
            read_array_from_buffer(ipc_buf, copy_handle_count, &mut ctx.out_params.copy_handles);
        ipc_buf =
            read_array_from_buffer(ipc_buf, move_handle_count, &mut ctx.out_params.move_handles);

        ipc_buf = ipc_buf.add(
            cmem::size_of::<SendStaticDescriptor>()
                * (*command_header).get_send_static_count() as usize,
        );
        ctx.out_params.data_words_offset = ipc_buf;
    }
}

#[inline(always)]
pub fn write_request_command_on_msg_buffer(
    ctx: &mut CommandContext,
    request_id: Option<u32>,
    domain_command_type: DomainCommandType,
) {
    unsafe {
        let ipc_buf = get_msg_buffer();

        let has_data_header = request_id.is_some();
        let mut data_size = DATA_PADDING + ctx.in_params.data_size;
        if has_data_header {
            data_size += cmem::size_of::<DataHeader>() as u32;
        }

        if ctx.object_info.is_domain() {
            data_size += (cmem::size_of::<DomainInDataHeader>()
                + cmem::size_of::<DomainObjectId>() * ctx.in_params.objects.len())
                as u32;
        }

        data_size = (data_size + 1) & !1;
        let out_pointer_sizes_offset = data_size;
        data_size += (cmem::size_of::<u16>() * ctx.in_params.out_pointer_sizes.len()) as u32;

        write_command_on_msg_buffer(ctx, CommandType::Request, data_size);
        let mut data_offset = get_aligned_data_offset(ctx.in_params.data_words_offset, ipc_buf);

        let out_pointer_sizes = ctx
            .in_params
            .data_words_offset
            .offset(out_pointer_sizes_offset as isize);
        write_array_to_buffer(
            out_pointer_sizes,
            ctx.in_params.out_pointer_sizes.len() as u32,
            &ctx.in_params.out_pointer_sizes,
        );

        let mut data_header = data_offset as *mut DataHeader;
        if ctx.object_info.is_domain() {
            let domain_header = data_offset as *mut DomainInDataHeader;
            let left_data_size = cmem::size_of::<DataHeader>() as u32 + ctx.in_params.data_size;
            *domain_header = DomainInDataHeader::new(
                domain_command_type,
                ctx.in_params.objects.len() as u8,
                left_data_size as u16,
                ctx.object_info.domain_object_id,
                0,
            );
            data_offset = data_offset.add(cmem::size_of::<DomainInDataHeader>());
            let objects_offset = data_offset.offset(left_data_size as isize);
            write_array_to_buffer(
                objects_offset,
                ctx.in_params.objects.len() as u32,
                &ctx.in_params.objects,
            );
            data_header = data_offset as *mut DataHeader;
        }

        if has_data_header {
            *data_header = DataHeader::new(IN_DATA_HEADER_MAGIC, 0, request_id.unwrap(), 0);
            data_offset = data_offset.add(cmem::size_of::<DataHeader>());
        }

        ctx.in_params.data_offset = data_offset;
    }
}

#[inline(always)]
pub fn read_request_command_response_from_msg_buffer(ctx: &mut CommandContext) -> Result<()> {
    unsafe {
        let ipc_buf = get_msg_buffer();
        read_command_response_from_msg_buffer(ctx);

        let mut data_offset = get_aligned_data_offset(ctx.out_params.data_words_offset, ipc_buf);
        let mut data_header = data_offset as *mut DataHeader;
        if ctx.object_info.is_domain() {
            let domain_header = data_offset as *mut DomainOutDataHeader;
            data_offset = data_offset.add(cmem::size_of::<DomainOutDataHeader>());
            let objects_offset =
                data_offset.add(cmem::size_of::<DataHeader>() + ctx.out_params.data_size as usize);
            let object_count = (*domain_header).out_object_count;
            read_array_from_buffer(objects_offset, object_count, &mut ctx.out_params.objects);
            data_header = data_offset as *mut DataHeader;
        }

        data_offset = data_offset.add(cmem::size_of::<DataHeader>());
        result_return_unless!(
            (*data_header).magic == OUT_DATA_HEADER_MAGIC,
            super::rc::ResultInvalidOutputHeader
        );
        result_try!(ResultCode::new((*data_header).value));

        ctx.out_params.data_offset = data_offset;
        Ok(())
    }
}

#[inline(always)]
pub fn write_control_command_on_msg_buffer(ctx: &mut CommandContext, request_id: ControlRequestId) {
    unsafe {
        let ipc_buf = get_msg_buffer();
        let data_size =
            DATA_PADDING + cmem::size_of::<DataHeader>() as u32 + ctx.in_params.data_size;

        write_command_on_msg_buffer(ctx, CommandType::Control, data_size);
        let mut data_offset = get_aligned_data_offset(ctx.in_params.data_words_offset, ipc_buf);

        let data_header = data_offset as *mut DataHeader;
        *data_header = DataHeader::new(IN_DATA_HEADER_MAGIC, 0, request_id as u32, 0);

        data_offset = data_offset.add(cmem::size_of::<DataHeader>());
        ctx.in_params.data_offset = data_offset;
    }
}

#[inline(always)]
pub fn read_control_command_response_from_msg_buffer(ctx: &mut CommandContext) -> Result<()> {
    unsafe {
        let ipc_buf = get_msg_buffer();

        read_command_response_from_msg_buffer(ctx);
        let mut data_offset = get_aligned_data_offset(ctx.out_params.data_words_offset, ipc_buf);

        let data_header = data_offset as *mut DataHeader;

        data_offset = data_offset.add(cmem::size_of::<DataHeader>());
        result_return_unless!(
            (*data_header).magic == OUT_DATA_HEADER_MAGIC,
            super::rc::ResultInvalidOutputHeader
        );
        result_try!(ResultCode::new((*data_header).value));

        ctx.out_params.data_offset = data_offset;
        Ok(())
    }
}

#[inline(always)]
pub fn write_close_command_on_msg_buffer(ctx: &mut CommandContext) {
    write_command_on_msg_buffer(ctx, CommandType::Close, 0);
}
