use super::*;
use core::mem as cmem;

#[inline(always)]
pub fn write_command_on_msg_buffer(ctx: &mut CommandContext, command_type: u32, data_size: u32) {
    unsafe {
        // TODO: in move handles are allowed?
        let mut ipc_buf = get_msg_buffer();

        let has_special_header = ctx.in_params.send_process_id
            || !ctx.in_params.copy_handles.is_empty()
            || !ctx.in_params.move_handles.is_empty();
        let data_word_count = data_size.div_ceil(4);
        let command_header = ipc_buf as *mut CommandHeader;
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
        ctx.out_params.data_words_offset = ipc_buf;
    }
}

#[inline(always)]
pub fn write_request_command_on_msg_buffer(ctx: &mut CommandContext, request_id: u32) {
    // TIPC directly sends the request ID here, without wasting data words
    let command_type = request_id + 16;
    write_command_on_msg_buffer(ctx, command_type, ctx.in_params.data_size);

    ctx.in_params.data_offset = ctx.in_params.data_words_offset;
}

#[inline(always)]
pub fn read_request_command_response_from_msg_buffer(ctx: &mut CommandContext) -> Result<()> {
    unsafe {
        read_command_response_from_msg_buffer(ctx);

        let data_offset = ctx.out_params.data_words_offset;
        let rc_ref = data_offset as *mut ResultCode;
        result_try!(*rc_ref);

        ctx.out_params.data_offset = rc_ref.offset(1) as *mut u8;
        Ok(())
    }
}

#[inline(always)]
pub fn write_close_command_on_msg_buffer(ctx: &mut CommandContext) {
    write_command_on_msg_buffer(ctx, CommandType::CloseSession as u32, 0);
}
