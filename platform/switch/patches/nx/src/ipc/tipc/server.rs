use super::*;
use core::mem as cmem;

#[inline(always)]
pub fn read_command_from_msg_buffer(ctx: &mut CommandContext) -> u32 {
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

        let send_buffer_count = (*command_header).get_send_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, send_buffer_count, &mut ctx.send_buffers);
        let receive_buffer_count = (*command_header).get_receive_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, receive_buffer_count, &mut ctx.receive_buffers);
        let exchange_buffer_count = (*command_header).get_exchange_buffer_count();
        ipc_buf = read_array_from_buffer(ipc_buf, exchange_buffer_count, &mut ctx.exchange_buffers);

        ctx.in_params.data_words_offset = ipc_buf;
        command_type
    }
}

#[inline(always)]
pub fn write_command_response_on_msg_buffer(
    ctx: &mut CommandContext,
    command_type: u32,
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
            command_type,
            0,
            ctx.send_buffers.len() as u32,
            ctx.receive_buffers.len() as u32,
            ctx.exchange_buffers.len() as u32,
            data_word_count,
            0,
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
    }
}

#[inline(always)]
pub fn read_request_command_from_msg_buffer(ctx: &mut CommandContext) -> Result<()> {
    let ipc_buf = get_msg_buffer();
    let data_offset = get_aligned_data_offset(ctx.in_params.data_words_offset, ipc_buf);

    ctx.in_params.data_offset = data_offset;
    Ok(())
}

#[inline(always)]
pub fn write_request_command_response_on_msg_buffer(
    ctx: &mut CommandContext,
    result: ResultCode,
    request_type: u32,
) {
    unsafe {
        let ipc_buf = get_msg_buffer();
        let data_size = cmem::size_of::<ResultCode>() as u32 + ctx.out_params.data_size;
        // data_size = (data_size + 1) & !1;

        write_command_response_on_msg_buffer(ctx, request_type, data_size);
        let data_offset = get_aligned_data_offset(ctx.out_params.data_words_offset, ipc_buf);
        let rc_ref = data_offset as *mut ResultCode;
        *rc_ref = result;

        ctx.out_params.data_offset = rc_ref.offset(1) as *mut u8;
    }
}

#[inline(always)]
pub fn write_close_command_response_on_msg_buffer(ctx: &mut CommandContext) {
    write_command_response_on_msg_buffer(ctx, CommandType::CloseSession as u32, 0);
}
